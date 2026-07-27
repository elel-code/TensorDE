use std::io;

pub(super) fn random_hex<const BYTES: usize>() -> io::Result<String> {
    let mut bytes = [0_u8; BYTES];
    let mut filled = 0;
    while filled < bytes.len() {
        match rustix::rand::getrandom(&mut bytes[filled..], rustix::rand::GetRandomFlags::empty()) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "getrandom returned no random-handle bytes",
                ));
            }
            Ok(read) => filled += read,
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
    }

    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut handle = String::with_capacity(BYTES * 2);
    for byte in bytes {
        handle.push(char::from(HEX[usize::from(byte >> 4)]));
        handle.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_handles_are_fixed_width_hex_and_distinct() {
        let first = random_hex::<32>().unwrap();
        let second = random_hex::<32>().unwrap();
        assert_eq!(first.len(), 64);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }
}
