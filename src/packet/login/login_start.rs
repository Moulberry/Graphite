use crate::binary_reader;

#[derive(Debug)]
pub struct LoginStart<'a> {
    pub username: &'a str,
    //pub signature: Option<Signature<'a>>
}

/*#[derive(Debug)]
pub struct Signature<'a> {
    pub timestamp: i64,
    pub public_key: &'a [u8],
    pub signature: &'a [u8]
}*/

impl <'a> LoginStart<'a> {
    pub fn read(bytes: &'a [u8]) -> anyhow::Result<LoginStart<'a>> {
        let mut bytes = bytes;

        let packet = LoginStart {
            username: binary_reader::read_string_with_max_size(&mut bytes, 16)?,
            //signature: Signature::read_optionally(bytes)?
        };

        binary_reader::ensure_fully_read(bytes)?;

        Ok(packet)
    }
}

/*impl <'a> Signature<'a> {
    fn read_optionally(bytes: &'a [u8]) -> anyhow::Result<Option<Signature<'a>>> {
        let mut bytes = bytes;

        if binary_reader::read_bool(&mut bytes)? {
            return Ok(Some(Signature::read(bytes)?));
        } else {
            return Ok(None);
        }
    }

    fn read(bytes: &'a [u8]) -> anyhow::Result<Signature<'a>> {
        let mut bytes = bytes;

        let signature = Signature {
            timestamp: binary_reader::read_i64(&mut bytes)?,
            public_key: binary_reader::read_sized_blob(&mut bytes)?,
            signature: binary_reader::read_sized_blob(&mut bytes)?,
        };

        Ok(signature)
    }
}*/