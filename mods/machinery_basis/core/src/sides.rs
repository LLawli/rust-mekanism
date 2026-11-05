//! Faces de um bloco e o que cada uma deixa entrar e sair.

/// As seis faces, na ordem do `Direction` do Horizon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Face {
    pub const ALL: [Face; 6] = [
        Face::Down,
        Face::Up,
        Face::North,
        Face::South,
        Face::West,
        Face::East,
    ];

    pub fn opposite(self) -> Face {
        match self {
            Face::Down => Face::Up,
            Face::Up => Face::Down,
            Face::North => Face::South,
            Face::South => Face::North,
            Face::West => Face::East,
            Face::East => Face::West,
        }
    }

    /// A face à direita de quem olha uma máquina de frente, como `RelativeSide.RIGHT`. Só faz
    /// sentido para faces horizontais.
    pub fn right_of(self) -> Face {
        match self {
            Face::North => Face::West,
            Face::West => Face::South,
            Face::South => Face::East,
            Face::East => Face::North,
            vertical => vertical,
        }
    }

    /// A face à esquerda de quem olha uma máquina de frente, como `RelativeSide.LEFT`.
    pub fn left_of(self) -> Face {
        self.right_of().opposite()
    }

    /// O deslocamento até o bloco vizinho por esta face.
    pub fn offset(self) -> [i32; 3] {
        match self {
            Face::Down => [0, -1, 0],
            Face::Up => [0, 1, 0],
            Face::North => [0, 0, -1],
            Face::South => [0, 0, 1],
            Face::West => [-1, 0, 0],
            Face::East => [1, 0, 0],
        }
    }

    /// O valor da propriedade `facing` de um bloco vanilla.
    pub fn from_property(value: &str) -> Option<Face> {
        match value {
            "down" => Some(Face::Down),
            "up" => Some(Face::Up),
            "north" => Some(Face::North),
            "south" => Some(Face::South),
            "west" => Some(Face::West),
            "east" => Some(Face::East),
            _ => None,
        }
    }
}

/// Os slots que uma face alcança.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FaceSlots {
    /// Onde um item que entra pode cair, na ordem de preferência.
    pub inputs: &'static [usize],
    /// De onde um item pode sair.
    pub outputs: &'static [usize],
}

/// Por quais faces uma máquina aceita entrada e entrega saída.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sides {
    pub input: [bool; 6],
    pub output: [bool; 6],
}

impl Sides {
    pub fn accepts_input(&self, face: Face) -> bool {
        self.input[face as usize]
    }

    pub fn gives_output(&self, face: Face) -> bool {
        self.output[face as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_undoes_itself() {
        for face in Face::ALL {
            assert_eq!(face.opposite().opposite(), face);
            assert_ne!(face.opposite(), face);
        }
    }

    #[test]
    fn opposite_offsets_cancel() {
        for face in Face::ALL {
            let [a, b, c] = face.offset();
            let [x, y, z] = face.opposite().offset();
            assert_eq!([a + x, b + y, c + z], [0, 0, 0]);
        }
    }

    #[test]
    fn right_goes_around_the_four_sides() {
        let mut face = Face::North;
        for _ in 0..4 {
            face = face.right_of();
        }
        assert_eq!(face, Face::North);
        assert_eq!(Face::North.right_of(), Face::West);
        assert_eq!(Face::Up.right_of(), Face::Up);
    }

    #[test]
    fn left_faces_the_right() {
        for face in [Face::North, Face::South, Face::West, Face::East] {
            assert_eq!(face.left_of(), face.right_of().opposite());
        }
        assert_eq!(Face::North.left_of(), Face::East);
    }

    #[test]
    fn unknown_property_is_no_face() {
        assert_eq!(Face::from_property("down"), Some(Face::Down));
        assert_eq!(Face::from_property("sideways"), None);
    }
}
