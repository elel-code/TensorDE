use tensor_kdl::Encode;

#[derive(Encode)]
union Bad {
    a: u32,
    b: f32,
}

fn main() {}
