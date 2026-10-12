use std::io::{self, Read, Write};

pub const READY: &[u8] = b"trache-trash/1";
pub const DELETE: u8 = b'D';
pub const SUCCESS: &[u8] = b"S";
pub const ERROR: u8 = b'E';
pub const MAX_FRAME: usize = 1024 * 1024;

pub fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub fn write_frame(writer: &mut impl Write, message: &[u8]) -> io::Result<()> {
    if message.is_empty() || message.len() > MAX_FRAME {
        return Err(invalid("invalid IPC frame size"));
    }
    writer.write_all(&(message.len() as u32).to_be_bytes())?;
    writer.write_all(message)?;
    writer.flush()
}

pub fn read_frame(reader: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut size = [0; 4];
    loop {
        match reader.read(&mut size[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    reader.read_exact(&mut size[1..])?;
    let size = u32::from_be_bytes(size) as usize;
    if size == 0 || size > MAX_FRAME {
        return Err(invalid("invalid IPC frame size"));
    }
    let mut message = vec![0; size];
    reader.read_exact(&mut message)?;
    Ok(Some(message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_binary_and_eof_is_distinct_from_truncation() {
        let message = b"D/tmp/a\xff\nfile";
        let mut bytes = Vec::new();
        write_frame(&mut bytes, message).unwrap();
        assert_eq!(read_frame(&mut &bytes[..]).unwrap().unwrap(), message);
        assert!(read_frame(&mut &[][..]).unwrap().is_none());
        for len in 1..bytes.len() {
            assert!(read_frame(&mut &bytes[..len]).is_err());
        }
        assert!(read_frame(&mut &u32::MAX.to_be_bytes()[..]).is_err());
        assert!(read_frame(&mut &[0; 4][..]).is_err());
    }
}
