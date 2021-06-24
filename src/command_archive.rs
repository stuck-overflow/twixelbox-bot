use crate::Cube;
use rusqlite::Connection;
use thiserror::Error;

// CubeArchive
//    new
//    getCubes -> Vec<Cube>
//    addCube
//    deleteCube
pub struct CubeArchive {
    sqlite_path: std::path::PathBuf,
    connection: Option<Connection>,
}

#[derive(Error, Debug)]
pub enum CubeArchiveError {
    #[error("error from rusqlite {0}")]
    Rusqlite(#[from] rusqlite::Error),
}

impl CubeArchive {
    pub fn new(sqlite_path: std::path::PathBuf) -> Self {
        Self {
            sqlite_path,
            connection: None,
        }
    }

    pub fn init(&mut self) -> Result<(), CubeArchiveError> {
        let conn = Connection::open(&self.sqlite_path)?;

        // create tables if not exist
        conn.execute(
            "create table if not exists cubes (
             x integer not null,
             y integer not null,
             z integer not null,
             r integer not null,
             g integer not null,
             b integer not null
         )",
            [],
        )?;
        self.connection = Some(conn);
        Ok(())
    }

    pub fn add_cube(&mut self, cube: Cube) -> Result<(), CubeArchiveError> {
        if self.connection.is_none() {
            self.init()?;
        }
        self.connection.as_ref().unwrap().execute(
            "INSERT INTO cubes (x, y, z, r, g, b) values (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                cube.position.0,
                cube.position.1,
                cube.position.2,
                cube.colour.0,
                cube.colour.1,
                cube.colour.2,
            ],
        )?;
        Ok(())
    }

    pub fn get_cubes(&mut self) -> Result<Vec<Cube>, CubeArchiveError> {
        if self.connection.is_none() {
            self.init()?;
        }
        let conn = self.connection.as_ref().unwrap();
        let mut stmt = conn.prepare("SELECT c.x, c.y, c.z, c.r, c.g, c.b from cubes c")?;

        let mapped_cubes = stmt.query_map([], |row| {
            Ok(Cube {
                position: (row.get(0)?, row.get(1)?, row.get(2)?),
                colour: (row.get(3)?, row.get(4)?, row.get(5)?),
            })
        })?;
        let mut cubes = Vec::<Cube>::new();
        for cube in mapped_cubes {
            cubes.push(cube?);
        }
        Ok(cubes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_add_get() {
        let sqlite_path = std::path::PathBuf::from(".testlite"); // TODO make it a tempfile
        let expected_cube = Cube {
            position: (0, 0, 0),
            colour: (0, 0, 0),
        };
        let mut archive = CubeArchive::new(sqlite_path.clone());
        archive.add_cube(expected_cube.clone()).unwrap();
        assert_eq!(archive.get_cubes().unwrap(), &[expected_cube.clone()][..]);

        let mut archive = CubeArchive::new(sqlite_path);
        assert_eq!(archive.get_cubes().unwrap(), &[expected_cube][..]);
    }
}
