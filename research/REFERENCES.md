# Research references

The referenced chat identifies three uploaded papers. Its attachment contents were unavailable in this session. The entries below use public author, publisher, and project versions. The RAMORT-specific adaptations and their proposed tests are in the [research proposal](RESEARCH_PROPOSAL.md) and [roadmap](ROADMAP.md).

## Potentials and certificates

- Jan Hoffmann and Steffen Jost, *Two decades of automatic amortized resource analysis*, Mathematical Structures in Computer Science 32 (2022), 729–759. Survey of cost semantics, potential-based typing, polynomial bases, and inference by numeric optimization. Corresponds to `two-decades-of-automatic-amortized-resource-analysis.pdf`. [Publisher](https://doi.org/10.1017/S0960129521000487)
- Quentin Carbonneaux, Jan Hoffmann, and Zhong Shao, *Compositional Certified Resource Bounds*, PLDI 2015. An imperative precedent for local potential obligations, function contracts, and independently checkable cost derivations. [Authors' paper](https://www.cs.cmu.edu/~janh/assets/pdf/CarbonneauxHZ15.pdf)

## Loops and recursion

- Marc Brockschmidt, Fabian Emmes, Stephan Falke, Carsten Fuhs, and Jürgen Giesl, *Alternating Runtime and Size Complexity Analysis of Integer Programs*, TACAS 2014. Provides the local-to-global argument for alternating bounds on counts and sizes. [Paper](https://verify.rwth-aachen.de/giesl/papers/TACAS14.pdf)
- KoAT project, tool documentation. Use the documented input format and analysis modes when designing a differential comparison. Record the release used in each experiment. [Documentation](https://koat.verify.rwth-aachen.de/docs/)
- Moritz Sinn, Florian Zuleger, and Helmut Veith, *Difference Constraints: An Adequate Abstraction for Complexity Analysis of Imperative Programs* (2015). Explains how increments and resets account for total work in dependent loops. [Authors' preprint](https://arxiv.org/abs/1508.04958)

## Rust borrows

- *Automatic Linear Resource Bound Analysis for Rust via Prophecy Potentials* (2025), RaRust. The paper develops linear resource reasoning for shared and mutable references, including future potential at the end of a loan. R-010 through R-013 investigate extending this method to richer potentials in RAMORT. Corresponds to `2502.19810v1.pdf`. [Version 1](https://arxiv.org/abs/2502.19810v1)
- Charon project. Its LLBC representation offers a comparison point for extracting borrow information from Rust. [Project repository](https://github.com/AeneasVerif/charon)

## Hybrid analysis and decomposition

- Long Pham, Feras A. Saad, and Jan Hoffmann, *Robust Resource Bounds with Static Analysis and Bayesian Inference*, PLDI 2024. Source for Hybrid AARA, BayesWC, BayesPC, and the assumptions governing statistical soundness. Corresponds to `hybrid-aara.pdf`. [Paper](https://doi.org/10.1145/3656380)
- Long Pham, *Hybrid Resource-Bound Analyses of Programs*, Carnegie Mellon University doctoral dissertation, CMU-CS-25-122 (2025). Chapter 8 develops resource decomposition. Chapter 9 addresses generation of high-cost inputs. [Dissertation](https://www.csd.cmu.edu/sites/default/files/phd-thesis/CMU-CS-25-122.pdf)
- Long Pham et al., *Integrating Resource Analyses via Resource Decomposition*, OOPSLA 2025. Gives a translation to resource-guarded programs and a soundness argument for symbolic substitution. Use its recursion-depth example when evaluating a Rust instantiation. [Paper](https://www.cs.cmu.edu/~janh/assets/pdf/PhamNGSH25.pdf) · [Evaluation artifact](https://zenodo.org/doi/10.5281/zenodo.15757837)
