pub mod kryptography;

use std::fmt;

use subtle::{Choice, ConstantTimeEq};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Zeroize, ZeroizeOnDrop)]
#[repr(transparent)]
pub struct Sensitive<const N: usize>([u8; N]);

impl<const N: usize> Sensitive<N> {
    #[inline]
    pub fn new(bytes: [u8; N]) -> Self {
        Self(bytes)
    }

    /// Devuelve una vista inmutable a los bytes (útil para tests).
    /// OJO: no expongas esto en APIs donde los bytes sean secretos si no es necesario.
    #[inline]
    pub fn as_bytes(&self) -> &[u8; N] {
        &self.0
    }

    /// Comparación en tiempo constante que devuelve `Choice`.
    /// No convierte a `bool` ni ramifica.
    #[inline]
    pub fn ct_eq(&self, other: &Self) -> Choice {
        self.0.ct_eq(&other.0)
    }
}

impl<const N: usize> fmt::Debug for Sensitive<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // No filtramos contenido
        write!(f, "Sensitive<{}>([REDACTED])", N)
    }
}

// --- Ejemplo de uso sin filtrar por control de flujo ---
#[cfg(test)]
mod example_usage {
    use super::*;
    #[test]
    fn example_ct_eq_api() {
        let a = Sensitive::<16>::new([0u8; 16]);
        let b = Sensitive::<16>::new([0u8; 16]);
        // Mantenemos Choice hasta lo último posible
        let _c: Choice = a.ct_eq(&b);
        // Si necesitas un bool (p.ej., para retornar desde una API pública no-CT),
        // conviértelo *fuera* del camino crítico:
        let _ok: bool = bool::from(_c);
        // No hay asserts del valor aquí, es sólo para compilar.
    }
}
