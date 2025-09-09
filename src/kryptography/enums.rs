// 2) Algoritmos por capacidad (no se mezclan)
#[derive(Clone, Copy, Debug)]
pub enum AeadAlg {
    AesGcm,
    ChaCha20Poly1305,
}

#[derive(Clone, Copy, Debug)]
pub enum StreamAlg {
    AesCtr, /*, ChaCha20*/
}

#[derive(Clone, Copy, Debug)]
pub enum BlockModeAlg {
    AesCbc, /*, CamelliaCbc*/
}
