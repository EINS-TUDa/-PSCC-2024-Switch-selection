# Switch selection





<div align="center">
    <img alt="Version" src="https://img.shields.io/badge/version_1.1.0-green?style=for-the-badge">
    <a href="https://github.com/EINS-TUDa/PSCC2024-SwitchSelection/blob/main/LICENSE.md">
        <img alt="Licence" src="https://img.shields.io/badge/licence-blue?style=for-the-badge">
    </a>
    <img alt="PSCC paper" src="https://img.shields.io/badge/pscc_paper_(link_coming_soon)-grey?style=for-the-badge">
</div>





## Contents

* [Brief](#brief)
* [Input format](#input-format)
* [Output format](#output-format)
* [What is GNBS?](#what-is-gnbs)
* [Available solvers](#available-solvers)
* [Compilation](#compilation)
* [Usage](#usage)
* [Benchmarking](#benchmarking)
    * [Reproduction of the results](#reproduction-of-the-results)
    * [Interpretation of the output](#interpretation-of-the-output)



## Brief

A tool in this repository solves the following problem.

**Input**

> * $G$ — a MV distribution grid that has a DG-kernel $D$ such that:
>     * $\text{V}(D)$ are all primary substations of $G$.
>     * There is an edge replacement sequence that constructs $G$ from $D$ with all replaced edges coming from $\text{E}(D)$.
>     * All radial subnetworks of $G$ are degenerate.
> * $p : \text{V}(G) \setminus \text{V}(D) \rightarrow \mathbb{Q}$ — active power at each secondary substation of $G$.
> * $q : \text{V}(G) \setminus \text{V}(D) \rightarrow \mathbb{Q}$ — reactive power at each secondary substation of $G$.
> * $r : \text{E}(G) \rightarrow \mathbb{Q}$ — resistance of each edge.
> * $x : \text{E}(G) \rightarrow \mathbb{Q}$ — reactance of each edge.

**Output**

> * $v : \text{V}(D) \rightarrow \{ -10, \dots, 10 \}$ — an optimal tap position for each primary substation.
> * $S \subseteq \text{E}(G)$ — set of edges where switches should be opened.

For further detail and definitions see our paper (link coming soon).



## Input format

The program expects a GNBS file as an input. The GNBS file must describe grid $G$ with following attributes defined:

|  Vertex attribute name  | Type | Meaning | Possible values |
|:-----------------------:|:----:|:--------|:----------------|
| `is primary substation` |  `B` | Flag of primary substations | `T` for primary substations, `F` for secondary substations |
| `p`                     | `F8` | Active power | A 64-bit float if `is primary substation == F`, `X` otherwise |
| `q`                     | `F8` | Reactive power | A 64-bit float if `is primary substation == F`, `X` otherwise |

| Edge attribute name | Type | Meaning | Possible values |
|:-------------------:|:----:|:--------|:----------------|
| `r`                 | `F8` | Resistance | A 64-bit float |
| `x`                 | `F8` | Reactance | A 64-bit float |



## Output format

The program produces a GNBS file as an output. This GNBS file is a full copy of the input file with the following additional attributes defined:

| Vertex attribute name | Type | Meaning | Possible values |
|:---------------------:|:----:|:--------|:----------------|
| `tap position`        | `I1` | Tap position $v$ that defines the base voltage $1 + 0.01 v$ | An integer from $\{ -10, \dots, 10 \}$ if `is primary substation == T`, `X` otherwise |

| Edge attribute name | Type | Meaning | Possible values |
|:-------------------:|:----:|:--------|:----------------|
| `opened switch`     | `B`  | Flag of an opened switch | `T` or `F` |



## What is GNBS?

You can find a full specification of GNBS format [here](https://github.com/jointpoints/GNBSFormat/blob/main/Specification.md).



## Available solvers

The following solvers are implemented in this tool:

* `TreeDecompositionSolver` — a solver that solves the problem with dynamic programming using tree decompositions.
* `CPLEXSolver` — a solver that solves the problem formulated as a MILP with the help of CPLEX.

To use the `CPLEXSolver` or to run the benchmark, you must have a copy of [CPLEX](https://www.ibm.com/products/ilog-cplex-optimization-studio/cplex-optimizer) installed on your computer. CPLEX is proprietary software owned by IBM. If you don't own a licence of CPLEX, you can still use our `TreeDecompositionSolver` without any problems or restrictions.



## Compilation

Clone this repository and run `build.py`, which is located in the root folder. Note that in order for compilation to be successful, the following must be present on your system:

* `python` — a Python interpreter, version 3, distributed either with pip or conda.
* `rustc` — a Rust compiler, version 1.76 or newer.
* `cargo` — a Rust build system and package manager, version 1.76 or newer.
* Internet connection.

The compiled program will be saved in the `Switch selection` folder, which is created automatically.



## Usage

The tool is supplied with a CLI. Run the following command to learn how to use it:

```
.\switch-selection.exe -h
```
on Windows or
```
switch-selection -h
```
on Linux.



## Benchmarking

#### Reproduction of the results

To reproduce the results from our PSCC paper, run

```
.\switch-selection.exe -b
```
on Windows or
```
switch-selection -b
```
on Linux. You must have a copy of CPLEX installed on your computer to run the benchmark.

#### Interpretation of the output

When launched in the benchmark mode, the program will print messages to the console. For each sample, the output will contain a line that looks something like `PDXPDXPDXPD` or, more generally, in the language of regular expressions, `(PDX)*PD`. Here is what these symbols mean:

* `P` — primary substations have been successfully generated.
* `D` — the corresponding distribution grid has been successfully generated.
* `X` — the generated sample turned out infeasible.

If the sample is found to be infeasible, the whole process starts again and is repeated until a feasible sample is produced. The time taken to identify infeasibility is not recorded and doesn't affect the metrics.
