use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;
use std::str::FromStr;
use twixelbox_bot::Cube;
use twixelbox_bot::CubeArchive;

#[derive(Debug)]
struct ChatCommand {
    x: i32,
    y: i32,
    z: i32,
    r: u8,
    g: u8,
    b: u8,
}

impl FromStr for ChatCommand {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let r: Result<Vec<_>, _> = value.split(' ').map(|v| v.parse::<i32>()).collect();
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

fn main() {
    if let Ok(lines) = read_lines("./test.ply") {
        // Consumes the iterator, returns an (Optional) String
        for line in lines {
            if let Ok(twixel_msg) = line {
                let chat_command = match twixel_msg.parse::<ChatCommand>() {
                    Err(_) => continue,
                    Ok(c) => c,
                };
                let chat_command = ChatCommand {
                    x: chat_command.x + 300,
                    y: chat_command.y + 50,
                    z: chat_command.z + 100,
                    ..chat_command
                };
                println!("{:?}", chat_command);
                // create a cube with the above params
                // TODO check that x, y, z are all > 0
                if chat_command.x < 0 || chat_command.y < 0 || chat_command.z < 0 {
                    continue;
                }

                let cube = Cube {
                    position: (
                        chat_command.x as u32,
                        chat_command.y as u32,
                        chat_command.z as u32,
                    ),
                    colour: (chat_command.r, chat_command.g, chat_command.b),
                };

                let sqlite_path = std::path::PathBuf::from("cube_archive.db");
                let mut archive = CubeArchive::new(sqlite_path.clone());
                archive.add_cube(cube.clone()).unwrap();
            }
        }
    }
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}
