# Related work

There are many different approaches to merging diverging files. The [awesome-merge-driver](https://github.com/jelmer/awesome-merge-drivers) list keeps track of implementations that can be used as Git merge drivers.
Some of them are flat, meaning that they break down the files as stream of lines or tokens (such as git's native diff3 algorithm). Others (like Mergiraf) are structured, representing the files as
abstract syntax trees and merging those trees using dedicated heuristics. We focus on the structured ones here.

There has also been some research around using machine-learning techniques or large language models to solve conflicts. We do not cover those, as none of them appear to be open source so far.

Among structured approaches focused on merging source code, we are aware of the following systems:


| System                                                                    | Languages        | Paper                                                                                                                                                                               | Year |
|---------------------------------------------------------------------------|------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------|
| [spork](https://github.com/ASSERT-KTH/spork)                              | Java             | [Spork: Structured Merge for Java with Formatting Preservation](https://arxiv.org/abs/2202.05329)                                                                                   | 2023 |
| [automerge-ptm](https://github.com/thufv/automerge-ptm)                   | Java             | [Enhancing Precision of Structured Merge by Proper Tree Matching](https://feihe.github.io/materials/icse19poster.pdf)                                                               | 2019 |
| [jsFSTMerge](https://github.com/AlbertoTrindade/jsFSTMerge)               | Javascript (ES5) | [Semistructured merge in JavaScript systems](https://repositorio.ufpe.br/bitstream/123456789/33477/1/DISSERTA%C3%87%C3%83O%20Alberto%20Trindade%20Tavares.pdf)                      | 2018 |
| [s3m](https://github.com/guilhermejccavalcanti/s3m)                       | Java             | [Evaluating and improving semistructured merge](https://pauloborba.cin.ufpe.br/publication/2017evaluating_and_improving_semistructured_merge/2017OOPSLASemiVsUnstructuredMerge.pdf) | 2017 |
| [jdime](https://github.com/se-sic/jdime)                                  | Java             | [Structured merge with auto-tuning: balancing precision and performance](https://www.se.cs.uni-saarland.de/publications/docs/ASE2012.pdf)                                           | 2012 |
| [FSTMerge](https://github.com/joliebig/featurehouse/tree/master/fstmerge) | Java, C#, Python | [Semistructured merge: rethinking merge in revision control systems](https://www.se.cs.uni-saarland.de/publications/docs/FSE2011.pdf)                                               | 2011 |

Also worth noting is [IntelliMerge](https://github.com/symbolk/intellimerge), which merges sets of files (instead of individual files, as is the case for all systems above).

## Overview of the differences with Spork

In this section we list the main ways in which Mergiraf differs from Spork, the existing system it is most similar to.

### Applicability to languages beyond Java

Spork is explicitly restricted to Java. This lies primarily in its reliance on Spoon, a Java parser, but also on some heuristics (such as detection of Java methods with identical signatures as an additional post-processing step). Instead, Mergiraf supports [a wider range of languages](./languages.md), similarly to FSTMerge.

### Better faithfulness to existing syntax

In certain cases, Spork normalizes some syntactic elements (such as adding brackets around sub-expressions or grouping together declarations of local variables of the same type). This can happen even if the elements are not involved in conflicting changes
and seems to be due to the Spoon parser.

In contrast to this, Mergiraf sticks to the original syntax (by the simple fact that it does not embed the language-specific knowledge to carry out such normalizations).

### Support for delete/modify conflicts

A delete/modify conflict is a situation where one side deletes an element while the other side makes changes to it.
In such cases, [Spork does not emit a conflict and just deletes the element](https://github.com/ASSERT-KTH/spork/issues/529). This can be a problem as the changes made by the other side are lost. In certain cases, those changes should instead be replayed
elsewhere (for instance, in a different file). For this reason, Mergiraf emits a conflict in such a case.

### Speed

Being a Java application, Spork is not well suited for use as a git merge driver. Such uses spawn one process per file to be merged, which causes a Java Virtual Machine to boot up each time. [Attempts to bundle it as a native executable have been inconclusive](https://github.com/ASSERT-KTH/spork/pull/481#issuecomment-1913688408).
In contrast to this, Mergiraf is a binary with much smaller start-up times, and its overall runtime is closer to that of Git's embedded merge algorithms.
