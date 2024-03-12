use crate::switch_selection_instance::{SwitchSelectionInstance, SwitchSelectionGraph};
use super::errors::SolverError;





pub type TapValue = i8;



pub trait BaseSolver: Sized {
    fn with_input(input: SwitchSelectionInstance) -> Result<Self, SolverError>;
    fn get_solution(&self) -> Option<(SwitchSelectionGraph, TapValue)>;
    fn solve(&mut self) -> Result<(), SolverError>;
}
