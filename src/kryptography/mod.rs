pub mod backends;
pub mod errors;
pub mod traits;

pub mod symmetric {
    use anyhow::Result;

    #[derive(Debug, Clone)]
    pub struct ModeByAlg {
        pub algorithm: Algorithm,
        pub modes: Vec<ModeKind>,
    }

    #[derive(Debug, Clone)]
    pub enum Algorithm {
        AES,
    }

    pub struct AES {}

    impl AES {
        pub fn encrypt(&self, data: &[u8], spec: Option<&CipherSpec>) -> Result<Vec<u8>> {
            Ok(data.to_vec())
        }

        pub fn decrypt(&self, data: &[u8], spec: Option<&CipherSpec>) -> Result<Vec<u8>> {
            Ok(data.to_vec())
        }
    }

    #[derive(Debug, Default)]
    pub struct Cipher {}

    impl Cipher {
        pub fn supported(&self) -> Vec<ModeByAlg> {
            vec![ModeByAlg {
                algorithm: Algorithm::AES,
                modes: vec![ModeKind::CBC, ModeKind::GCM],
            }]
        }

        pub fn create(&self, spec: &CipherSpec) -> Result<AES> {
            Ok(AES {})
        }
    }

    #[derive(Debug, Clone)]
    pub enum Padding {
        Pkcs7,
    }

    #[derive(Debug, Clone)]
    pub enum Mode {
        CBC {
            iv: Vec<u8>,
            padding: Padding,
        },
        GCM {
            iv: Vec<u8>,
            tag: Option<Vec<u8>>,
            min_tag_len: usize,
        },
    }

    #[derive(Debug, Clone)]
    pub enum ModeKind {
        CBC,
        GCM,
    }

    #[derive(Debug, Clone)]
    pub struct CipherSpec {
        pub algorithm: Algorithm,
        pub key: Vec<u8>,
        pub mode: Mode,
    }
}
