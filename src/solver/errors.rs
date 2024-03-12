use std::{error::Error, fmt::Display};





#[derive(Debug)]
pub struct GraphError {
    description: String,
}

// GraphError::GraphError
impl GraphError {
    #[inline]
    pub fn from_str(description: &str) -> Self {
        GraphError { description: description.to_string() }
    }

    #[inline]
    pub fn from_string(description: String) -> Self {
        GraphError { description }
    }
}

// GraphError::Error
impl Error for GraphError {}

// GraphError::Display
impl Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description)
    }
}



#[derive(Debug)]
pub struct SolverError {
    description: String,
}

// SolverError::SolverError
impl SolverError {
    #[inline]
    pub fn from_str(description: &str) -> Self {
        SolverError { description: description.to_string() }
    }

    #[inline]
    pub fn from_string(description: String) -> Self {
        SolverError { description }
    }
}

// SolverError::Error
impl Error for SolverError {}

// SolverError::Display
impl Display for SolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description)
    }
}
