use thiserror::Error;

/// Application-specific error types
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Rendering error: {0}")]
    Render(#[from] RenderError),

    #[error("Background task error: {0}")]
    Background(#[from] BackgroundError),

    #[error("GPU error: {0}")]
    Gpu(String),

    #[error("Shader compilation error: {0}")]
    Shader(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Generic error: {0}")]
    Generic(#[from] anyhow::Error),
}

/// Rendering-specific errors
#[derive(Error, Debug)]
pub enum RenderError {
    #[error("Failed to initialize wgpu: {0}")]
    WgpuInit(String),

    #[error("Failed to create render pipeline: {0}")]
    Pipeline(String),

    #[error("Failed to create texture: {0}")]
    Texture(String),

    #[error("Shader compilation failed: {0}")]
    ShaderCompilation(String),
}

/// Background task errors
#[derive(Error, Debug)]
pub enum BackgroundError {
    #[error("Task execution failed: {0}")]
    Execution(String),

    #[error("Task was cancelled")]
    Cancelled,

    #[error("Task timeout")]
    Timeout,
}

/// Type alias for Results using our error types
pub type Result<T> = anyhow::Result<T>;
pub type AppResult<T> = std::result::Result<T, AppError>;
