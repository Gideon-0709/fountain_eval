# Build composition

`src/build.rs` implements maintainer-owned composition. Users select two component references and a test version; they do not write a target JSON or Cargo manifest.

Public native integration binds registered scheme/utility 2.1.0. Development and product integrations use the selected monorepo's corresponding paths. Original and effective manifests are recorded; declared version constraints are checked before source binding. Product library and callback dependency graphs remain separate.

Sources, lockfiles, generated projects and compiled artifacts live in the ignored cache. Native feature mapping and dependency closure are C/I provenance, not common E equality requirements. Unmapped workspace patches, ambiguous implementations and incompatible dependencies fail explicitly.
