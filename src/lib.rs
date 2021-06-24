mod command_archive;

pub use command_archive::CubeArchive;

#[derive(Clone, Debug, PartialEq)]
pub struct Cube {
    pub position: (u32, u32, u32),
    pub colour: (u8, u8, u8),
}
