# Safer build options

By default,
the Rust compiler includes your current path information
in the binaries that it generates.
This could be a problem if,
for example, you are building from a path like
`/home/FirstnameLastname/build/arti`
and releasing binaries (or uploading backtraces)
under a pseudonym
that you do not want linked to `FirstnameLastname`.

There is a good overview of the issues here at
https://github.com/betrusted-io/xous-core/issues/57 .

There are a couple of workarounds here.

# Workaround one: reproducible build

If you have Docker,
you can run a reproducible build of Arti,
so that the binary you make will be the same
as a binary generated by anybody else.

See the
[`docker_reproducible_build`](../maint/docker_reproducible_build)
script for more information.

# Workaround two: RUSTFLAGS

As a quick-and-dirty solution,
you can use the `--remap-path-prefix` option
to tell the Rust compiler
to re-map your paths into anonymized ones.

This is not a perfect solution;
there are known issues under some configurations,
particularly if you are linking to a static OpenSSL.

Personally, I get good results from running:

```
RUSTFLAGS="--remap-path-prefix $HOME/.cargo=.cargo --remap-path-prefix $(pwd)=." \
   cargo build --release -p arti
```

After you do this, you can use
`strings target/release/arti | grep "$HOME"`
to see if your home directory appears in the result.





