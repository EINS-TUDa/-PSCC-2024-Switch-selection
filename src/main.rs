mod switch_selection_instance;
mod tree_decomposition;
mod solver;

use std::{env, process::exit, time::Instant};
use crabnets::{io::IO, Graph};
use switch_selection_instance::{SwitchSelectionInstance, SwitchSelectionGraph};
use solver::{base_solver::BaseSolver, cplex_solver::CPLEXSolver, tree_decomposition_solver::TreeDecompositionSolver, benchmark::start_benchmark};
use crate::solver::base_solver::TapValue;





macro_rules! pretty_panic {
    ($message: expr) => {
        {
            println!("{}", $message);
            exit(1);
        }
    };
}

macro_rules! pretty_unwrap {
    ($result: expr) => {
        match $result {
            Ok(value) => value,
            Err(error) => pretty_panic!(error),
        }
    };
}





fn main() {
    const HELP_STRING: &str =
"switch-selection

A tool to solve a variation of the SwitchSelection problem described in our  PSCC  paper.  See  our
repository https://github.com/EINS-TUDa/PSCC2024-SwitchSelection for a detailed user guide.

USAGE
    OPTION 1: switch-selection [...] (-h|--help) [...]
    OPTION 2: switch-selection (-b|--benchmark)
    OPTION 3: switch-selection <ARGUMENTS> [<OPTIONS>]

    OPTION 1 prints this message. OPTION 2 launches benchmark: use it to reproduce the results from
    our paper. Use OPTION 3 to solve a specific instance of the SwitchSelection problem.

ARGUMENTS
    (-i|--input) PATH        Set the path to the input file in GNBS format.
                             Default value if this argument is omitted: -i input.gnbs
    (-o|--output) PATH       Set the path to the output file in GNBS format. If  the  file  doesn't
                             exist, it'll  be  created  automatically.  If  the  file  exists,  its
                             contents will be rewritten.
                             Default value if this argument is omitted: -o output.gnbs
    (-s|--solver) SOLVER     Set a solver to solve the problem instance with. Possible  values  for
                             SOLVER:
                                    o  TreeDecompositionSolver  -  solve  the  problem  using   the
                                            dynamic programming approach described in our paper.
                                    o  CPLEXSolver  -  solve the problem in  its  MILP  formulation
                                            using CPLEX (requires CPLEX to be installed).
                             Default value if this argument is omitted: -s TreeDecompositionSolver

OPTIONS
    --dgkernel [PATH]        Save a DG-kernel of the input graph into a GNBS file. If PATH  is  not
                             given, value 'dgkernel.gnbs' is assumed.

EXAMPLES
    switch-selection
        Equivalent to 'switch-selection -i input.gnbs -o output.gnbs -s TreeDecompositionSolver'.
    switch-selection -i ./Graphs/example1.gnbs -s CPLEXSolver
        Solve the SwitchSelection instance given by ./Graphs/example1.gnbs  with  CPLEX,  save  the
        optimal solution into output.gnbs.
    switch-selection -o 123.gnbs --dgkernel dgk.gnbs
        Solve the SwitchSelection instance given by input.gnbs with  TreeDecompositionSolver,  save
        the optimal solution into 123.gnbs and save the DG-kernel into dgk.gnbs.";



    let mut benchmark_mode: Option<bool> = None;
    let mut input_path: String = "input.gnbs".to_string();
    let mut output_path: String = "output.gnbs".to_string();
    let mut solver_name: String = "TreeDecompositionSolver".to_string();
    let mut dg_kernel_path: Option<String> = None;
    let mut timeit: Option<(usize, usize)> = None;
    let mut target_path: &mut String = &mut input_path;
    // Command-line parser states
    enum CLParserState {
        ExpectParameter,
        ExpectPath,
        ExpectPathOrParameter,
        ExpectSolver,
        ExpectNumber1OrParameter,
        ExpectNumber2,
    }



    // Read command-line arguments
    let mut state: CLParserState = CLParserState::ExpectParameter;
    let mut arguments: env::Args = env::args();
    arguments.next();
    for argument in arguments {
        match argument.as_str() {
            "-b" | "--benchmark" => if benchmark_mode == Some(false) {
                pretty_panic!(format!("You can't use {} together with any other parameters.", argument));
            } else {
                benchmark_mode = Some(true);
            },
            "-h" | "--help" => {
                println!("{}", HELP_STRING);
                exit(0);
            },
            "-i" | "--input" => match state {
                CLParserState::ExpectParameter | CLParserState::ExpectPathOrParameter | CLParserState::ExpectNumber1OrParameter => {
                    if benchmark_mode == Some(true) {
                        pretty_panic!(format!("You can't use {} in the benchmark mode.", argument));
                    }
                    benchmark_mode = Some(false);
                    target_path = &mut input_path;
                    state = CLParserState::ExpectPath;
                },
                _ => pretty_panic!(format!("Unexpected command-line value {}.", argument)),
            },
            "-o" | "--output" => match state {
                CLParserState::ExpectParameter | CLParserState::ExpectPathOrParameter | CLParserState::ExpectNumber1OrParameter => {
                    if benchmark_mode == Some(true) {
                        pretty_panic!(format!("You can't use {} in the benchmark mode.", argument));
                    }
                    benchmark_mode = Some(false);
                    target_path = &mut output_path;
                    state = CLParserState::ExpectPath;
                },
                _ => pretty_panic!(format!("Unexpected command-line value {}.", argument)),
            },
            "-s" | "--solver" => match state {
                CLParserState::ExpectParameter | CLParserState::ExpectPathOrParameter | CLParserState::ExpectNumber1OrParameter => {
                    if benchmark_mode == Some(true) {
                        pretty_panic!(format!("You can't use {} in the benchmark mode.", argument));
                    }
                    benchmark_mode = Some(false);
                    state = CLParserState::ExpectSolver;
                },
                _ => pretty_panic!(format!("Unexpected command-line value {}.", argument)),
            },
            "--dgkernel" => match state {
                CLParserState::ExpectParameter | CLParserState::ExpectNumber1OrParameter => {
                    if benchmark_mode == Some(true) {
                        pretty_panic!(format!("You can't use {} in the benchmark mode.", argument));
                    }
                    benchmark_mode = Some(false);
                    dg_kernel_path = Some("dgkernel.gnbs".to_string());
                    state = CLParserState::ExpectPathOrParameter;
                },
                _ => pretty_panic!(format!("Unexpected command-line value {}.", argument)),
            },
            "--timeit" => match state {
                CLParserState::ExpectParameter | CLParserState::ExpectPathOrParameter => {
                    if benchmark_mode == Some(true) {
                        pretty_panic!(format!("You can't use {} in the benchmark mode.", argument));
                    }
                    benchmark_mode = Some(false);
                    timeit = Some((100, 10));
                    state = CLParserState::ExpectNumber1OrParameter;
                },
                _ => pretty_panic!(format!("Unexpected command-line value {}.", argument)),
            },
            a => match state {
                CLParserState::ExpectParameter => pretty_panic!(format!("Unknown command-line parameter {}.", a)),
                CLParserState::ExpectPath => {
                    *target_path = argument;
                    state = CLParserState::ExpectParameter;
                },
                CLParserState::ExpectPathOrParameter => {
                    dg_kernel_path = Some(a.to_string());
                    state = CLParserState::ExpectParameter;
                }
                CLParserState::ExpectSolver => {
                    match a {
                        "TreeDecompositionSolver" | "CPLEXSolver" => solver_name = a.to_string(),
                        _ => pretty_panic!(format!("Unknown solver {}.", a)),
                    }
                    state = CLParserState::ExpectParameter;
                },
                CLParserState::ExpectNumber1OrParameter => {
                    timeit = Some((pretty_unwrap!(a.parse()), 0));
                    state = CLParserState::ExpectNumber2;
                },
                CLParserState::ExpectNumber2 => {
                    timeit = Some((timeit.unwrap().0, pretty_unwrap!(a.parse())));
                    state = CLParserState::ExpectParameter;
                },
            },
        }
    }
    match state {
        CLParserState::ExpectParameter | CLParserState::ExpectPathOrParameter | CLParserState::ExpectNumber1OrParameter => (),
        _ => pretty_panic!(format!("Unexpected end of command line.")),
    }



    // Act according to the parameters set
    if benchmark_mode == Some(true) {
        pretty_unwrap!(start_benchmark(2, 10, 30, 20, 1, 0));
    } else {
        let input: SwitchSelectionGraph = pretty_unwrap!(Graph::from_file(&input_path));
        let problem_instance: SwitchSelectionInstance = pretty_unwrap!(SwitchSelectionInstance::new(input));
        if let Some(value) = dg_kernel_path {
            pretty_unwrap!(problem_instance.dg_kernel_for_switch_selection().into_file(&value));
        }
        let solver_begin_time: Instant = Instant::now();
        match solver_name.as_str() {
            "TreeDecompositionSolver" => {
                let mut solver: TreeDecompositionSolver = pretty_unwrap!(TreeDecompositionSolver::with_input(problem_instance));
                pretty_unwrap!(solver.solve());
                println!("{} solved the problem instance in {} s.", solver_name, solver_begin_time.elapsed().as_secs_f64());
                let solution: (SwitchSelectionGraph, TapValue) = solver.get_solution().unwrap();
                println!("Objective value = {}.", solution.1);
                pretty_unwrap!(solution.0.into_file(&output_path));
            },
            "CPLEXSolver" => {
                let mut solver: CPLEXSolver = pretty_unwrap!(CPLEXSolver::with_input(problem_instance));
                pretty_unwrap!(solver.solve());
                println!("{} solved the problem instance in {} s.", solver_name, solver_begin_time.elapsed().as_secs_f64());
                let solution: (SwitchSelectionGraph, TapValue) = solver.get_solution().unwrap();
                println!("Objective value = {}.", solution.1);
                pretty_unwrap!(solution.0.into_file(&output_path));
            },
            _ => (),
        }
    }
}
