use tensor_kdl::Decode;

#[derive(Decode)]
struct Bad {
    #[kdl(not_a_real_attr)]
    x: i64,
}

fn main() {}
