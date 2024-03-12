use std::{collections::HashMap, pin::Pin, ptr::NonNull};
use cplex_dynamic::{Constraint, ConstraintType, Env, Problem, ProblemType, Solution, Variable, VariableType, VariableValue, WeightedVariable};
use crabnets::{BasicImmutableGraph, BasicMutableGraph, ImmutableGraphContainer};
use crate::switch_selection_instance::{SwitchSelectionGraph, SwitchSelectionInstance};
use super::{base_solver::*, errors::SolverError};





macro_rules! cplex_unwrap {
    ($expr: expr) => {
        match $expr {
            Ok(value) => value,
            Err(error) => return Err(SolverError::from_string(format!("CPLEXSolver. {}", error))),
        }
    };
}



pub struct CPLEXSolverCore<'a> {
    input: SwitchSelectionInstance,
    solution: Option<Solution>,
    variables: HashMap<String, usize>,
    problem: Option<Problem<'a>>,
    env: Env,
}



pub type CPLEXSolver<'a> = Pin<Box<CPLEXSolverCore<'a>>>;



trait CPLEXSolverTools<'a> {
    fn get_problem_mut(&mut self) -> &mut Problem<'a>;
}



// CPLEXSolver::CPLEXSolverTools
impl<'a> CPLEXSolverTools<'a> for CPLEXSolver<'a> {
    fn get_problem_mut(&mut self) -> &mut Problem<'a> {
        unsafe {
            self.as_mut().get_unchecked_mut().problem.as_mut().unwrap()
        }
    }
}

// CPLEXSolver::BaseSolver
impl<'a> BaseSolver for CPLEXSolver<'a> {
    fn with_input(input: SwitchSelectionInstance) -> Result<Self, SolverError> {
        let mut solver: CPLEXSolver = Box::pin(CPLEXSolverCore { input: input.clone(), env: cplex_unwrap!(Env::new()), variables: HashMap::new(), problem: None, solution: None });
        unsafe {
            // Create a problem instance
            solver.as_mut().get_unchecked_mut().problem = Some(cplex_unwrap!(Problem::new(NonNull::from(&solver.as_ref().env).as_ref(), "name")));
        }
        // Populate the problem with variables
        // * max_tap_abs                             : {0, ..., 10}   -- max |tap(s)| = max tap_abs(s) over all s in P(input)
        // * u(s) for s in S(input)                  : [0.81, 1.21]   -- square voltage at substation s
        // * tap(i, s) for s in P(input)             : {0, 1}         -- whether tap at a primary substation s is in position i in {-10, ..., 10}
        // * tap_abs(s) for s in P(input)            : {0, ..., 10}   -- |tap(s)| = 1 * (tap(-1, s) + tap(1, s)) + ... + 10 * (tap(-10, s) + tap(10, s))
        // * part(s) for s in S(input) \ P(input)    : {0, 1}         -- shows to which primary substation s should be attributed to: left (0) or right (1)
        // * u_right(s) for s in S(input) \ P(input) : [0.0, 1.21]    -- an alias for part(s) * u(s') where s' is the substation to the right of s
        // * u_left(s) for s in S(input) \ P(input)  : [0.0, 1.21]    -- an alias for (1 - part(s)) * u(s') where s' is the substation to the left of s
        let mut variables: HashMap<String, usize> = HashMap::new();
        variables.insert(
            "max_tap_abs".to_string(),
            cplex_unwrap!(solver.get_problem_mut().add_variable(Variable::new(VariableType::Integer, 1.0, 0.0, 10.0, "max_tap_abs")))
        );
        for substation_id in input.iter_v() {
            variables.insert(
                format!("u({})", substation_id),
                cplex_unwrap!(solver.get_problem_mut().add_variable(Variable::new(VariableType::Continuous, 0.0, 0.81, 1.21, format!("u({})", substation_id))))
            );
            if input.v_attrs(&substation_id).unwrap().tap_position.is_some() {
                for tap_position in -10..=10 {
                    variables.insert(
                        format!("tap({},{})", tap_position, substation_id),
                        cplex_unwrap!(solver.get_problem_mut().add_variable(Variable::new(VariableType::Integer, 0.0, 0.0, 1.0, format!("tap({},{})", tap_position, substation_id))))
                    );
                }
                variables.insert(
                    format!("tap_abs({})", substation_id),
                    cplex_unwrap!(solver.get_problem_mut().add_variable(Variable::new(VariableType::Integer, 0.0, 0.0, 10.0, format!("tap_abs({})", substation_id))))
                );
            } else {
                variables.insert(
                    format!("part({})", substation_id),
                    cplex_unwrap!(solver.get_problem_mut().add_variable(Variable::new(VariableType::Integer, 0.0, 0.0, 1.0, format!("part({})", substation_id))))
                );
                variables.insert(
                    format!("u_right({})", substation_id),
                    cplex_unwrap!(solver.get_problem_mut().add_variable(Variable::new(VariableType::Continuous, 0.0, 0.0, 1.21, format!("u_right({})", substation_id))))
                );
                variables.insert(
                    format!("u_left({})", substation_id),
                    cplex_unwrap!(solver.get_problem_mut().add_variable(Variable::new(VariableType::Continuous, 0.0, 0.0, 1.21, format!("u_left({})", substation_id))))
                );
            }
        }
        // Traverse each line, add the remaining variables...
        // * right_part(s1, s2) for s1, s2 in S(input) \ P(input) : {0, 1}  -- an alias for part(s1) * part(s2)
        // * left_part(s1, s2) for s1, s2 in S(input) \ P(input)  : {0, 1}  -- an alias for (1 - part(s1)) * (1 - part(s2))
        // ... and add all necessary constraints
        let mut constraint: Constraint;
        for primary_substation_id in input.dg_kernel_for_switch_selection().iter_v() {
            // * u(s) = <one-hot encoding of squared voltages>
            {
                // Possible values of the square voltage at a primary substation.
                // Tap positions are: T = {-10, ..., 10}.
                // Base voltage: B = {1 + 0.1 * t | t \in T}.
                // Square base voltage: {u² | u \in B}; for each t \in T the  squared  base
                // voltage corresponding to t is BASE_VOLTAGE_SQ[t + 10].
                const BASE_VOLTAGE_SQ: [f64; 21] = [0.81, 0.8281, 0.8464, 0.8649, 0.8836, 0.9025, 0.9216, 0.9409, 0.9604, 0.9801, 1.0,
                                                    1.0201, 1.0404, 1.0609, 1.0816, 1.1025, 1.1236, 1.1449, 1.1664, 1.1881, 1.21];
                constraint = Constraint::new(ConstraintType::Eq, 0.0, format!("u({})", primary_substation_id));
                constraint.add_wvar(WeightedVariable::new_idx(variables[&format!("u({})", primary_substation_id)], 1.0));
                for tap_position in -10..=10i8 {
                    constraint.add_wvar(WeightedVariable::new_idx(variables[&format!("tap({},{})", tap_position, primary_substation_id)], -BASE_VOLTAGE_SQ[(tap_position + 10) as usize]));
                }
                cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
            }
            // * tap(-10, s) + ... + tap(10, s) = 1
            {
                constraint = Constraint::new(ConstraintType::Eq, 1.0, format!("sum_tap({})", primary_substation_id));
                for tap_position in -10..=10i8 {
                    constraint.add_wvar(WeightedVariable::new_idx(variables[&format!("tap({},{})", tap_position, primary_substation_id)], 1.0));
                }
                cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
            }
            // * tap_abs(s) = 1 * (tap(-1, s) + tap(1, s)) + ... + 10 * (tap(-10, s) + tap(10, s))
            {
                constraint = Constraint::new(ConstraintType::Eq, 0.0, format!("tap_abs({})", primary_substation_id));
                constraint.add_wvar(WeightedVariable::new_idx(variables[&format!("tap_abs({})", primary_substation_id)], 1.0));
                for tap_position in -10..=10i8 {
                    constraint.add_wvar(WeightedVariable::new_idx(variables[&format!("tap({},{})", tap_position, primary_substation_id)], -(tap_position.abs()) as f64));
                }
                cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
            }
            // * max_tap_abs >= tap_abs(s)
            {
                constraint = Constraint::new(ConstraintType::GreaterThanEq, 0.0, format!("max_tap_abs({})", primary_substation_id));
                constraint.add_wvar(WeightedVariable::new_idx(variables[&"max_tap_abs".to_string()], 1.0));
                constraint.add_wvar(WeightedVariable::new_idx(variables[&format!("tap_abs({})", primary_substation_id)], -1.0));
                cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
            }
            for adjacent_id in input.iter_adjacent(&primary_substation_id).unwrap() {
                let endpoints = input.e_attrs(&primary_substation_id, &adjacent_id, &0).unwrap().line_endpoints.unwrap();
                if input.v_attrs(&adjacent_id).unwrap().line_endpoints.is_some() && endpoints.1 != primary_substation_id {
                    // Collect the line
                    // We can guarantee here that line.len() >= 3
                    let mut line: Vec<usize> = vec![primary_substation_id, adjacent_id];
                    while *line.last().unwrap() != endpoints.1 {
                        for line_neighbour_id in input.iter_adjacent(line.last().unwrap()).unwrap() {
                            if line_neighbour_id == endpoints.1
                            || line[line.len() - 2] != line_neighbour_id {
                                line.push(line_neighbour_id);
                                break;
                            }
                        }
                    }
                    // Process all secondary substations on the line
                    for substation_i1 in 1..=(line.len() - 2) {
                        // * u(s_j) = u_right(s_j) + sum_{k = 1}^j [ ( - r(s_j, s_j+1) p(s_k) + x(s_j, s_j+1) q(s_k) ) right_part(s_k, s_j) ] 
                        //          +  u_left(s_j) + sum_{k = j}^m [ ( - r(s_j-1, s_j) p(s_k) + x(s_j-1, s_j) q(s_k) ) left_part(s_j, s_k) ]
                        {
                            constraint = Constraint::new(
                                ConstraintType::Eq,
                                0,
                                format!("powerbalance({})", line[substation_i1])
                            );
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("u({})", line[substation_i1])],
                                -1.0
                            ));
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("u_right({})", line[substation_i1])],
                                1.0
                            ));
                            // sum for u_right(s_j)
                            for substation_i2 in 1..=substation_i1 {
                                variables.insert(
                                    format!("right_part({},{})", line[substation_i2], line[substation_i1]),
                                    cplex_unwrap!(solver.get_problem_mut().add_variable(Variable::new(VariableType::Integer, 0.0, 0.0, 1.0, format!("right_part({},{})", line[substation_i2], line[substation_i1]))))
                                );
                                constraint.add_wvar(WeightedVariable::new_idx(
                                    variables[&format!("right_part({},{})", line[substation_i2], line[substation_i1])],
                                    - input.e_attrs(&line[substation_i1], &line[substation_i1 + 1], &0).unwrap().r
                                    * input.v_attrs(&line[substation_i2]).unwrap().p
                                    + input.e_attrs(&line[substation_i1], &line[substation_i1 + 1], &0).unwrap().x
                                    * input.v_attrs(&line[substation_i2]).unwrap().q
                                ));
                            }
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("u_left({})", line[substation_i1])],
                                1.0
                            ));
                            // sum for u_left(s_j)
                            for substation_i2 in substation_i1..=(line.len() - 2) {
                                variables.insert(
                                    format!("left_part({},{})", line[substation_i1], line[substation_i2]),
                                    cplex_unwrap!(solver.get_problem_mut().add_variable(Variable::new(VariableType::Integer, 0.0, 0.0, 1.0, format!("left_part({},{})", line[substation_i2], line[substation_i1]))))
                                );
                                constraint.add_wvar(WeightedVariable::new_idx(
                                    variables[&format!("left_part({},{})", line[substation_i1], line[substation_i2])],
                                    - input.e_attrs(&line[substation_i1 - 1], &line[substation_i1], &0).unwrap().r
                                    * input.v_attrs(&line[substation_i2]).unwrap().p
                                    + input.e_attrs(&line[substation_i1 - 1], &line[substation_i1], &0).unwrap().x
                                    * input.v_attrs(&line[substation_i2]).unwrap().q
                                ));
                            }
                            cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
                        }
                        // * u_right(s_j) = part(s_j) * u(s_j+1), which is linearised as
                        // ----* u_right(s_j) >= 0.81 * part(s_j)
                        {
                            constraint = Constraint::new(
                                ConstraintType::GreaterThanEq,
                                0.0,
                                format!("u_right({})_lin1", line[substation_i1])
                            );
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("u_right({})", line[substation_i1])],
                                1.0
                            ));
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("part({})", line[substation_i1])],
                                -0.81
                            ));
                            cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
                        }
                        // ----* u_right(s_j) <= 1.21 * part(s_j)
                        {
                            constraint = Constraint::new(
                                ConstraintType::LessThanEq,
                                0.0,
                                format!("u_right({})_lin2", line[substation_i1])
                            );
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("u_right({})", line[substation_i1])],
                                1.0
                            ));
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("part({})", line[substation_i1])],
                                -1.21
                            ));
                            cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
                        }
                        // ----* u_right(s_j) <= u(s_j+1) - 2 part(s_j) + 2
                        {
                            constraint = Constraint::new(
                                ConstraintType::LessThanEq,
                                2.0,
                                format!("u_right({})_lin3", line[substation_i1])
                            );
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("u_right({})", line[substation_i1])],
                                1.0
                            ));
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("u({})", line[substation_i1 + 1])],
                                -1.0
                            ));
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("part({})", line[substation_i1])],
                                2.0
                            ));
                            cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
                        }
                        // ----* u_right(s_j) >= u(s_j+1) + 2 part(s_j) - 2
                        {
                            constraint = Constraint::new(
                                ConstraintType::GreaterThanEq,
                                -2.0,
                                format!("u_right({})_lin4", line[substation_i1])
                            );
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("u_right({})", line[substation_i1])],
                                1.0
                            ));
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("u({})", line[substation_i1 + 1])],
                                -1.0
                            ));
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("part({})", line[substation_i1])],
                                -2.0
                            ));
                            cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
                        }
                        // * u_left(s_j) = (1 - part(s_j)) * u(s_j-1), which is linearised as
                        // ----* u_left(s_j) >= -0.81 * part(s_j) + 0.81
                        {
                            constraint = Constraint::new(
                                ConstraintType::GreaterThanEq,
                                0.81,
                                format!("u_left({})_lin1", line[substation_i1])
                            );
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("u_left({})", line[substation_i1])],
                                1.0
                            ));
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("part({})", line[substation_i1])],
                                0.81
                            ));
                            cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
                        }
                        // ----* u_left(s_j) <= -1.21 * part(s_j) + 1.21
                        {
                            constraint = Constraint::new(
                                ConstraintType::LessThanEq,
                                1.21,
                                format!("u_left({})_lin2", line[substation_i1])
                            );
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("u_left({})", line[substation_i1])],
                                1.0
                            ));
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("part({})", line[substation_i1])],
                                1.21
                            ));
                            cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
                        }
                        // ----* u_left(s_j) <= u(s_j-1) + 2 * part(s_j)
                        {
                            constraint = Constraint::new(
                                ConstraintType::LessThanEq,
                                0.0,
                                format!("u_left({})_lin3", line[substation_i1])
                            );
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("u_left({})", line[substation_i1])],
                                1.0
                            ));
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("u({})", line[substation_i1 - 1])],
                                -1.0
                            ));
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("part({})", line[substation_i1])],
                                -2.0
                            ));
                            cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
                        }
                        // ----* u_left(s_j) >= u(s_j-1) - 2 * part(s_j)
                        {
                            constraint = Constraint::new(
                                ConstraintType::GreaterThanEq,
                                0.0,
                                format!("u_left({})_lin4", line[substation_i1])
                            );
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("u_left({})", line[substation_i1])],
                                1.0
                            ));
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("u({})", line[substation_i1 - 1])],
                                -1.0
                            ));
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("part({})", line[substation_i1])],
                                2.0
                            ));
                            cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
                        }
                        // * part(s_j-1) <= part(s_j)
                        if substation_i1 > 1 {
                            constraint = Constraint::new(
                                ConstraintType::LessThanEq,
                                0.0,
                                format!("part({})", line[substation_i1])
                            );
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("part({})", line[substation_i1 - 1])],
                                1.0
                            ));
                            constraint.add_wvar(WeightedVariable::new_idx(
                                variables[&format!("part({})", line[substation_i1])],
                                -1.0
                            ));
                            cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
                        }
                        // Constraints for right_part(s_k, s_j)
                        for substation_i2 in 1..=substation_i1 {
                            // * right_part(s_k, s_j) = part(s_k) * part(s_j), which is liearised as
                            // ----* right_part(s_k, s_j) <= part(s_k)
                            {
                                constraint = Constraint::new(
                                    ConstraintType::LessThanEq,
                                    0.0,
                                    format!("right_part({},{})_lin1", line[substation_i2], line[substation_i1])
                                );
                                constraint.add_wvar(WeightedVariable::new_idx(
                                    variables[&format!("right_part({},{})", line[substation_i2], line[substation_i1])],
                                    1.0
                                ));
                                constraint.add_wvar(WeightedVariable::new_idx(
                                    variables[&format!("part({})", line[substation_i2])],
                                    -1.0
                                ));
                                cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
                            }
                            // ----* right_part(s_k, s_j) <= part(s_j)
                            {
                                constraint = Constraint::new(
                                    ConstraintType::LessThanEq,
                                    0.0,
                                    format!("right_part({},{})_lin2", line[substation_i2], line[substation_i1])
                                );
                                constraint.add_wvar(WeightedVariable::new_idx(
                                    variables[&format!("right_part({},{})", line[substation_i2], line[substation_i1])],
                                    1.0
                                ));
                                constraint.add_wvar(WeightedVariable::new_idx(
                                    variables[&format!("part({})", line[substation_i1])],
                                    -1.0
                                ));
                                cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
                            }
                            // ----* right_part(s_k, s_j) >= part(s_k) + part(s_j) - 1
                            {
                                constraint = Constraint::new(
                                    ConstraintType::GreaterThanEq,
                                    -1.0,
                                    format!("right_part({},{})_lin3", line[substation_i2], line[substation_i1])
                                );
                                constraint.add_wvar(WeightedVariable::new_idx(
                                    variables[&format!("right_part({},{})", line[substation_i2], line[substation_i1])],
                                    1.0
                                ));
                                if substation_i1 == substation_i2 {
                                    constraint.add_wvar(WeightedVariable::new_idx(
                                        variables[&format!("part({})", line[substation_i1])],
                                        -2.0
                                    ));
                                } else {
                                    constraint.add_wvar(WeightedVariable::new_idx(
                                        variables[&format!("part({})", line[substation_i2])],
                                        -1.0
                                    ));
                                    constraint.add_wvar(WeightedVariable::new_idx(
                                        variables[&format!("part({})", line[substation_i1])],
                                        -1.0
                                    ));
                                }
                                cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
                            }
                        }
                        // Constraints for left_part(s_j, s_k)
                        for substation_i2 in substation_i1..=(line.len() - 2) {
                            // * left_part(s_j, s_k) = (1 - part(s_j)) * (1 - part(s_k)), which is linearised as
                            // ----* left_part(s_j, s_k) <= 1 - part(s_j)
                            {
                                constraint = Constraint::new(
                                    ConstraintType::LessThanEq,
                                    1.0,
                                    format!("left_part({},{})_lin1", line[substation_i1], line[substation_i2])
                                );
                                constraint.add_wvar(WeightedVariable::new_idx(
                                    variables[&format!("left_part({},{})", line[substation_i1], line[substation_i2])],
                                    1.0
                                ));
                                constraint.add_wvar(WeightedVariable::new_idx(
                                    variables[&format!("part({})", line[substation_i1])],
                                    1.0
                                ));
                                cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
                            }
                            // ----* left_part(s_j, s_k) <= 1 - part(s_k)
                            {
                                constraint = Constraint::new(
                                    ConstraintType::LessThanEq,
                                    1.0,
                                    format!("left_part({},{})_lin2", line[substation_i1], line[substation_i2])
                                );
                                constraint.add_wvar(WeightedVariable::new_idx(
                                    variables[&format!("left_part({},{})", line[substation_i1], line[substation_i2])],
                                    1.0
                                ));
                                constraint.add_wvar(WeightedVariable::new_idx(
                                    variables[&format!("part({})", line[substation_i2])],
                                    1.0
                                ));
                                cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
                            }
                            // ----* left_part(s_j, s_k) >= - part(s_j) - part(s_k) + 1
                            {
                                constraint = Constraint::new(
                                    ConstraintType::GreaterThanEq,
                                    1.0,
                                    format!("left_part({},{})_lin3", line[substation_i1], line[substation_i2])
                                );
                                constraint.add_wvar(WeightedVariable::new_idx(
                                    variables[&format!("left_part({},{})", line[substation_i1], line[substation_i2])],
                                    1.0
                                ));
                                if substation_i1 == substation_i2 {
                                    constraint.add_wvar(WeightedVariable::new_idx(
                                        variables[&format!("part({})", line[substation_i1])],
                                        2.0
                                    ));
                                } else {
                                    constraint.add_wvar(WeightedVariable::new_idx(
                                        variables[&format!("part({})", line[substation_i1])],
                                        1.0
                                    ));
                                    constraint.add_wvar(WeightedVariable::new_idx(
                                        variables[&format!("part({})", line[substation_i2])],
                                        1.0
                                    ));
                                }
                                cplex_unwrap!(solver.get_problem_mut().add_constraint(constraint));
                            }
                        }
                    }
                }
            }
        }
        solver.variables = variables;
        Ok(solver)
    }

    fn get_solution(&self) -> Option<(SwitchSelectionGraph, TapValue)> {
        if self.solution.is_none() {
            return None;
        }
        let mut answer = self.input.unwrap().clone();
        for primary_substation_id in self.input.dg_kernel_for_switch_selection().iter_v() {
            for tap_position in -10..10i8 {
                if let VariableValue::Integer(1) = self.solution.as_ref().unwrap().variables[self.variables[&format!("tap({},{})", tap_position, primary_substation_id)]] {
                    answer.v_attrs_mut(&primary_substation_id).unwrap().tap_position = Some(tap_position);
                }
            }
        }
        for edge in self.input.iter_e() {
            let endpoints = self.input.e_attrs(&edge.id1, &edge.id2, &0).unwrap().line_endpoints.unwrap();
            if self.input.v_attrs(&edge.id1).unwrap().tap_position.is_some() {
                if self.input.v_attrs(&edge.id2).unwrap().tap_position.is_some() {
                    answer.e_attrs_mut(&edge.id1, &edge.id2, &0).unwrap().switch = true;
                    continue;
                }
                if let VariableValue::Integer(value) = self.solution.as_ref().unwrap().variables[self.variables[&format!("part({})", edge.id2)]] {
                    answer.e_attrs_mut(&edge.id1, &edge.id2, &0).unwrap().switch = endpoints.0 == edge.id1 && value == 1 || endpoints.1 == edge.id1 && value == 0;
                }
                continue;
            }
            if self.input.v_attrs(&edge.id2).unwrap().tap_position.is_some() {
                if let VariableValue::Integer(value) = self.solution.as_ref().unwrap().variables[self.variables[&format!("part({})", edge.id1)]] {
                    answer.e_attrs_mut(&edge.id1, &edge.id2, &0).unwrap().switch = endpoints.0 == edge.id2 && value == 1 || endpoints.1 == edge.id2 && value == 0;
                }
                continue;
            }
            if let VariableValue::Integer(value1) = self.solution.as_ref().unwrap().variables[self.variables[&format!("part({})", edge.id1)]] {
                if let VariableValue::Integer(value2) = self.solution.as_ref().unwrap().variables[self.variables[&format!("part({})", edge.id2)]] {
                    answer.e_attrs_mut(&edge.id1, &edge.id2, &0).unwrap().switch = value1 != value2;
                }
            }
        }
        Some((answer, self.solution.as_ref().unwrap().objective as TapValue))
    }

    fn solve(&mut self) -> Result<(), SolverError> {
        self.solution = Some(cplex_unwrap!(self.get_problem_mut().solve(ProblemType::MixedInteger)));
        Ok(())
    }
}
