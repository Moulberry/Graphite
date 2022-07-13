use std::fmt::Debug;



#[derive(Debug, Clone, Copy)]
pub struct Coordinate {
    pub x: f64,
    pub y: f64,
    pub z: f64
}

#[derive(Debug, Clone, Copy)]
pub struct Rotation {
    pub yaw: f32,
    pub pitch: f32
}

#[derive(Clone, Copy)]
pub struct Position {
    pub coord: Coordinate,
    pub rot: Rotation
}

impl Debug for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Position")
            .field("x", &self.coord.x)
            .field("y", &self.coord.y)
            .field("z", &self.coord.z)
            .field("yaw", &self.rot.yaw)
            .field("pitch", &self.rot.pitch)
            .finish()
    }
}