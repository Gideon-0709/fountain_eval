# Native drivers

No native driver or I version is registered yet. Expected integration families are public engine 2.1, development engine 3 and the binary product API.

Future drivers may use independent Cargo build roots and lockfiles for API isolation. They are not extra public framework crates and must not introduce native Fountain dependencies into the root package.

API conversions belong to I; measurement semantics executed by drivers belong to T. Equivalent I implementations can be compared under the same T/E. Equivalence must be verified, not inferred from names or matching signatures.
