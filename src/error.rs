//! Error type shared across all metrics.

/// Errors produced when constructing images or computing metrics.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The two images passed to a metric have different dimensions.
    #[error("dimension mismatch: {a:?} vs {b:?}")]
    DimensionMismatch {
        /// Dimensions of the first (reference) image, as `(width, height)`.
        a: (u32, u32),
        /// Dimensions of the second (distorted) image, as `(width, height)`.
        b: (u32, u32),
    },

    /// An image is smaller than the minimum a metric can process.
    #[error("image too small: {0}x{1}, minimum is {2}x{2}")]
    ImageTooSmall(u32, u32, u32),

    /// A pixel buffer's length does not match the declared dimensions.
    #[error("buffer size mismatch: expected {expected} bytes, got {actual}")]
    BufferSize {
        /// Number of bytes the dimensions/format require.
        expected: usize,
        /// Number of bytes actually provided.
        actual: usize,
    },

    /// Two images are otherwise valid but cannot be compared directly
    /// (mismatched channel layout, bit depth, or color space).
    #[error("incompatible images: {0}")]
    Incompatible(String),

    /// The native SSIMULACRA2 implementation reported a failure.
    #[error("ssimulacra2 computation failed")]
    Ssimulacra2Failed,
}

/// Convenience alias for results returned by this crate.
pub type Result<T> = std::result::Result<T, Error>;
