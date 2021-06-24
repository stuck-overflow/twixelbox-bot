mod token_storage;

extern crate kiss3d;
extern crate nalgebra as na;

use image::RgbImage;
use kiss3d::light::Light;
use kiss3d::window::Window;
use log::{debug, trace, LevelFilter};
use na::Translation3;
use serde::Deserialize;
use simple_logger::SimpleLogger;
use std::fs;
use std::str::FromStr;
use structopt::StructOpt;
use tempfile::tempdir;
use token_storage::CustomTokenStorage;
use tokio::sync::mpsc;
use twitch_api2::twitch_oauth2::Scope;
use twitch_irc::login::{RefreshingLoginCredentials, TokenStorage};
use twitch_irc::message::ServerMessage;
use twitch_irc::{ClientConfig, TCPTransport, TwitchIRCClient};
use twixelbox_bot::Cube;
use twixelbox_bot::CubeArchive;

#[derive(Clone, Deserialize)]
struct TwixelBoxBotConfig {
    twitch: TwitchConfig,
    twixelbox: TwixelBoxConfig,
}

#[derive(Clone, Deserialize)]
struct TwitchConfig {
    token_filepath: String,
    login_name: String,
    channel_name: String,
    client_id: String,
    secret: String,
}

#[derive(Clone, Deserialize)]
struct TwixelBoxConfig {
    window_resolution: u32,
    cube_size: u32,
    img_filepath: String,
}

// Command-line arguments for the tool.
#[derive(StructOpt)]
struct Cli {
    /// Log level
    #[structopt(short, long, case_insensitive = true, default_value = "INFO")]
    log_level: LevelFilter,

    /// Twitch credential files.
    #[structopt(short, long, default_value = "twixelbox-bot.toml")]
    config_file: String,
}

struct Canvas {
    frame_side_len: u32,
}

impl Canvas {
    fn add_cube(
        &mut self,
        window: &mut Window,
        x: u32,
        y: u32,
        z: u32,
        r: u8,
        g: u8,
        b: u8,
    ) -> kiss3d::scene::SceneNode {
        // TODO: what if the cube already exists? store all the cubes and if it already exists only
        // set_color on existing cube.
        // TODO: check x, y, z < frame_side_len or bail out

        let voxel_side_len = 1.0 / self.frame_side_len as f32;
        let mut voxel = window.add_cube(voxel_side_len, voxel_side_len, voxel_side_len);
        voxel.set_color(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
        // x = [0.25 (leftmost), -0.25 (rightmost)]
        // y = [0.25 (upmost), -0.25 (downmost)]
        // z = [0.25 (backmost), -0.25 (frontmost)]

        let x =
            ((self.frame_side_len as f32 - x as f32) / (self.frame_side_len as f32 / 0.5)) - 0.25;
        let y =
            ((self.frame_side_len as f32 - y as f32) / (self.frame_side_len as f32 / 0.5)) - 0.25;
        let z =
            ((self.frame_side_len as f32 - z as f32) / (self.frame_side_len as f32 / 0.5)) - 0.25;
        voxel.append_translation(&Translation3::new(x, y, z));
        voxel
    }

    // TODO: do we need to add a remove_cube?
}

#[derive(Debug)]
enum Command {
    Render,
    AddCube(Cube),
}

#[derive(Debug)]
struct ChatCommand {
    x: u32,
    y: u32,
    z: u32,
    r: u8,
    g: u8,
    b: u8,
}

impl FromStr for ChatCommand {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let r: Result<Vec<_>, _> = value.split(' ').map(|v| v.parse::<u32>()).collect();
        match r {
            Ok(v) => {
                if v.len() != 6usize {
                    return Err("too many args");
                }
                if v[3] > 255 || v[4] > 255 || v[5] > 255 {
                    return Err("invalid r g b");
                }
                Ok(ChatCommand {
                    x: v[0],
                    y: v[1],
                    z: v[2],
                    r: v[3] as u8,
                    g: v[4] as u8,
                    b: v[5] as u8,
                })
            }
            Err(_) => Err("error parsing"),
        }
    }
}

#[tokio::main]
pub async fn main() {
    let args = Cli::from_args();
    SimpleLogger::new()
        .with_level(args.log_level)
        .init()
        .unwrap();

    let config = match fs::read_to_string(&args.config_file) {
        Ok(config) => config,
        Err(e) => {
            eprintln!(
                "Error opening the configuration file {}: {}",
                args.config_file, e
            );
            eprintln!("Create the file or use the --config_file flag to specify an alternative file location");
            return;
        }
    };

    let config: TwixelBoxBotConfig = match toml::from_str(&config) {
        Ok(config) => config,
        Err(e) => {
            eprintln!(
                "Error parsing configuration file {}: {}",
                args.config_file, e
            );
            return;
        }
    };

    let mut token_storage = CustomTokenStorage {
        token_checkpoint_file: config.twitch.token_filepath.clone(),
    };

    // If we have some errors while loading the stored token, e.g. if we never
    // stored one before or it's unparsable, go through the authentication
    // workflow.
    if let Err(_) = token_storage.load_token().await {
        let user_token = match twitch_oauth2_auth_flow::auth_flow_surf(
            &config.twitch.client_id,
            &config.twitch.secret,
            Some(vec![Scope::ChatRead]),
            "http://localhost:10666/twitch/token",
        ) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Error during the authentication flow: {}", e);
                return;
            }
        };
        token_storage
            .write_twitch_oauth2_user_token(
                &user_token,
                Some(oauth2::ClientSecret::new(config.twitch.secret.clone())),
            )
            .unwrap();
    }

    let irc_config = ClientConfig::new_simple(RefreshingLoginCredentials::new(
        config.twitch.login_name.clone(),
        config.twitch.client_id.clone(),
        config.twitch.secret.clone(),
        token_storage.clone(),
    ));

    let (mut incoming_messages, twitch_irc_client) =
        TwitchIRCClient::<TCPTransport, _>::new(irc_config);

    // join a channel
    twitch_irc_client.join(config.twitch.channel_name.to_owned());

    // Window initialisation.
    let window_size_pixels = 1080;
    let mut window =
        Window::new_with_size("Kiss3d: points", window_size_pixels, window_size_pixels);

    // TODO: fake the wireframe so that diagonals are not rendered.
    let mut c = window.add_cube(0.5, 0.5, 0.5);

    c.set_color(0.99, 0.99, 0.99);
    c.set_points_size(10.0);
    c.set_lines_width(0.1);
    c.set_surface_rendering_activation(false);

    window.set_light(Light::StickToCamera);
    window.set_background_color(250.0 / 255.0, 250.0 / 255.0, 250.0 / 255.0);

    let mut canvas = Canvas {
        frame_side_len: 500,
    };

    // Set up the channel to send commands to the main thread which controls the canvas.
    let (tx, mut rx) = mpsc::unbounded_channel::<Command>();

    let fps: f32 = 0.5;

    // Spawn the renderer timer thread.
    let tx2 = tx.clone();
    tokio::spawn(async move {
        let frame_time_millis = std::time::Duration::from_millis((1000.0 / fps) as u64);
        loop {
            // send render message
            tx.send(Command::Render).unwrap();
            tokio::time::sleep(frame_time_millis).await;
        }
    });

    // Message processing thread.
    let cube_size = config.twixelbox.cube_size;
    tokio::spawn(async move {
        while let Some(message) = incoming_messages.recv().await {
            trace!("{:?}", message);
            match message {
                ServerMessage::Privmsg(msg) => {
                    let chat_command = match msg.message_text.parse::<ChatCommand>() {
                        Err(_) => continue,
                        Ok(c) => c,
                    };
                    debug!("{:?}", chat_command);
                    if [chat_command.x, chat_command.y, chat_command.z]
                        .iter()
                        .any(|p| p >= &cube_size)
                    {
                        continue;
                    }

                    debug!("{:?} sending", chat_command);
                    tx2.send(Command::AddCube(Cube {
                        position: (chat_command.x, chat_command.y, chat_command.z),
                        colour: (chat_command.r, chat_command.g, chat_command.b),
                    }))
                    .unwrap();
                }
                _ => continue,
            }
        }
    });

    // Read previous cubes from db and add the to the canvas.

    let sqlite_path = std::path::PathBuf::from("cube_archive.db");
    let mut archive = CubeArchive::new(sqlite_path.clone());
    let cubes = archive.get_cubes().expect("failed to extract cubes");
    for cube in cubes {
        canvas.add_cube(
            &mut window,
            cube.position.0,
            cube.position.1,
            cube.position.2,
            cube.colour.0,
            cube.colour.1,
            cube.colour.2,
        );
    }

    let frame_time = std::time::Duration::from_millis((1000.0 / fps) as u64);
    let mut next_expected_frame = std::time::Instant::now();
    // The main thread now only receives commands and alters the canvas as required.
    while let Some(command) = rx.recv().await {
        match command {
            Command::Render => {
                let current_time = std::time::Instant::now();
                if current_time < next_expected_frame {
                    eprintln!(
                        "skipping frame at {:?}, next expected at {:?}",
                        current_time, next_expected_frame
                    );
                    continue;
                }
                let mut v = Vec::new();
                window.render();
                window.snap(&mut v);
                match RgbImage::from_raw(window_size_pixels, window_size_pixels, v) {
                    Some(img) => {
                        let tmpdir = tempdir().unwrap();
                        let tmpfile = tmpdir.path().join("img.png");
                        if let Err(e) = img.save(&tmpfile) {
                            eprintln!("Unable to save to tmpfile: {}", e);
                            continue;
                        }

                        fs::rename(tmpfile, &config.twixelbox.img_filepath).unwrap();
                    }
                    None => eprintln!("Unable to convert pixels to RgbImage!"),
                }
                let last_attempted_frame = current_time;
                let current_time = std::time::Instant::now();
                let render_time = current_time.duration_since(last_attempted_frame);
                let frames_lost = render_time.as_millis() / frame_time.as_millis();
                if frames_lost > 0 {
                    eprintln!(
                        "Lost {} frames! Render time {:?}, frame time {:?}",
                        frames_lost, render_time, frame_time
                    );
                }
                next_expected_frame = last_attempted_frame
                    .checked_add(
                        frame_time
                            .checked_mul(frames_lost as u32 + 1)
                            .expect("Failed to compute lost frames"),
                    )
                    .expect("Failed to compute next expected frame");
            }
            Command::AddCube(cube) => {
                canvas.add_cube(
                    &mut window,
                    cube.position.0,
                    cube.position.1,
                    cube.position.2,
                    cube.colour.0,
                    cube.colour.1,
                    cube.colour.2,
                );
                archive
                    .add_cube(cube)
                    .expect("Failed to add cube to database");
            }
        }
    }
}
