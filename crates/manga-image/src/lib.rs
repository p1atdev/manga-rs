//! Image processing for acquired bytes. This crate does not perform network I/O.
mod direct;
pub use direct::*;
#[cfg(any(feature = "fuz", feature = "kadokomi"))]
mod solver;
#[cfg(any(feature = "fuz", feature = "kadokomi"))]
mod utils {
    pub type Bytes = Vec<u8>;
}
#[doc(hidden)]
pub mod viewer {
    #[cfg(feature = "comici")]
    pub mod comici {
        pub mod data;
    }
    #[cfg(feature = "direct-sort")]
    pub mod giga {
        pub mod direct_sort;
    }
    #[cfg(feature = "magapoke")]
    pub mod magapoke;
    #[cfg(feature = "kadokomi")]
    pub mod kadocomi {
        pub mod solver;
    }
    #[cfg(feature = "fuz")]
    pub mod fuz {
        pub mod crypto;
        pub mod solver;
    }
}
