# Build composition

Reserved for maintainer-owned integration/build recipes. Users select engine/operator references directly and do not write target JSON.

Recipes will define compatible source composition and support dependencies, including monorepo paths and product bindings. Record actual dependencies and reject incompatible combinations. Snapshots and generated build roots belong in ignored work/cache directories, not here.
