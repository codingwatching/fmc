use std::slice::Iter;

use bevy::prelude::*;

use crate::constants::*;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum Direction {
    // Forward is +z direction
    Forward,
    Back,
    Right,
    Left,
    Up,
    Down,
    None,
}

impl Direction {
    // TODO: Is it better to use an associated constant for the normals?
    pub fn normal(&self) -> Vec3 {
        match self {
            &Direction::Forward => Vec3::Z,
            &Direction::Back => -Vec3::Z,
            &Direction::Right => Vec3::X,
            &Direction::Left => -Vec3::X,
            &Direction::Up => Vec3::Y,
            &Direction::Down => -Vec3::Y,
            &Direction::None => panic!("Can't get normal of None side"),
        }
    }

    pub fn opposite(&self) -> Self {
        match self {
            &Direction::Forward => Direction::Back,
            &Direction::Back => Direction::Forward,
            &Direction::Right => Direction::Left,
            &Direction::Left => Direction::Right,
            &Direction::Up => Direction::Down,
            &Direction::Down => Direction::Up,
            &Direction::None => panic!("Can't get opposite of None side"),
        }
    }

    pub fn is_opposite(&self, check_opposing: &Self) -> bool {
        match self {
            &Direction::Forward => check_opposing == &Direction::Back,
            &Direction::Back => check_opposing == &Direction::Forward,
            &Direction::Right => check_opposing == &Direction::Left,
            &Direction::Left => check_opposing == &Direction::Right,
            &Direction::Up => check_opposing == &Direction::Down,
            &Direction::Down => check_opposing == &Direction::Up,
            &Direction::None => panic!("Can't get opposite of None side"),
        }
    }

    /// Finds the Directions orthogonal to self.
    pub fn surrounding(&self) -> Vec<Self> {
        let mut all = vec![
            Self::Forward,
            Self::Back,
            Self::Left,
            Self::Right,
            Self::Up,
            Self::Down,
        ];
        all.retain(|dir| dir != &self.opposite() && dir != self);
        return all;
    }

    /// Find the Directions that are orthogonal on each Direction in 'surrounding', but that do not
    /// go in the direction of self, or its opposite.
    pub fn orthogonal(&self, surrounding: &Vec<Self>) -> Vec<[Self; 2]> {
        let mut ortho = Vec::with_capacity(4);
        for side in surrounding {
            if self == &Direction::Up || self == &Direction::Down {
                match side {
                    Direction::Right => ortho.push([Direction::Forward, Direction::Back]),
                    Direction::Left => ortho.push([Direction::Forward, Direction::Back]),
                    Direction::Forward => ortho.push([Direction::Left, Direction::Right]),
                    Direction::Back => ortho.push([Direction::Left, Direction::Right]),
                    _ => {}
                }
            } else if self == &Direction::Forward || self == &Direction::Back {
                match side {
                    Direction::Right => ortho.push([Direction::Up, Direction::Down]),
                    Direction::Left => ortho.push([Direction::Up, Direction::Down]),
                    Direction::Up => ortho.push([Direction::Left, Direction::Right]),
                    Direction::Down => ortho.push([Direction::Left, Direction::Right]),
                    _ => {}
                }
            } else if self == &Direction::Right || self == &Direction::Left {
                match side {
                    Direction::Forward => ortho.push([Direction::Up, Direction::Down]),
                    Direction::Back => ortho.push([Direction::Up, Direction::Down]),
                    Direction::Up => ortho.push([Direction::Forward, Direction::Back]),
                    Direction::Down => ortho.push([Direction::Forward, Direction::Back]),
                    _ => {}
                }
            }
        }
        return ortho;
    }

    /// Moves a position a chunk's length in self's Direction
    pub fn shift_chunk_position(&self, mut position: IVec3) -> IVec3 {
        match self {
            Direction::Forward => position.z += CHUNK_SIZE as i32,
            Direction::Back => position.z -= CHUNK_SIZE as i32,
            Direction::Right => position.x += CHUNK_SIZE as i32,
            Direction::Left => position.x -= CHUNK_SIZE as i32,
            Direction::Up => position.y += CHUNK_SIZE as i32,
            Direction::Down => position.y -= CHUNK_SIZE as i32,
            Direction::None => {}
        }
        return position;
    }

    /// Return the direction of a vector.
    pub fn convert_vector(vec: &Vec3) -> Self {
        let abs = vec.abs();
        if abs.x > abs.y && abs.x > abs.z {
            if vec.x < 0.0 {
                return Direction::Left;
            } else {
                return Direction::Right;
            }
        } else if abs.y > abs.x && abs.y > abs.z {
            if vec.y < 0.0 {
                return Direction::Down;
            } else {
                return Direction::Up;
            }
        } else {
            if vec.z < 0.0 {
                return Direction::Back;
            } else {
                return Direction::Forward;
            }
        }
    }

    /// Given a postion that is 1 block outside of a chunk, return which direction it is in.
    pub fn convert_position(pos: &IVec3) -> Self {
        if pos.z > (CHUNK_SIZE - 1) as i32 {
            return Direction::Forward;
        } else if pos.z < 0 {
            return Direction::Back;
        } else if pos.x > (CHUNK_SIZE - 1) as i32 {
            return Direction::Right;
        } else if pos.x < 0 {
            return Direction::Left;
        } else if pos.y > (CHUNK_SIZE - 1) as i32 {
            return Direction::Up;
        } else if pos.y < 0 {
            return Direction::Down;
        } else {
            return Direction::None;
        }
    }

    pub fn iter() -> Iter<'static, Direction> {
        use self::Direction::*;
        static DIRECTIONS: [Direction; 6] = [Forward, Back, Right, Left, Up, Down];
        DIRECTIONS.iter()
    }
}
