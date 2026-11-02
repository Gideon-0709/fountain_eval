# Build composition

`src/build.rs` implements maintainer-owned composition. Users supply engine, operator, scheme and the complete content version in one config; they do not write a target JSON or Cargo manifest.

Public native integration defaults to registered scheme/utility 2.1.0. Native configs may explicitly select the scheme source; Cargo metadata must confirm its identity. Product configs reject external scheme substitution. Development and product integrations use the selected monorepo's corresponding paths. Original and effective manifests are recorded; declared version constraints are checked before source binding. Product library and callback dependency graphs remain separate.

Sources, lockfiles, generated projects and compiled artifacts live in the ignored cache. Native feature mapping and dependency closure are C/I provenance, not common E equality requirements. Unmapped workspace patches, ambiguous implementations and incompatible dependencies fail explicitly.
