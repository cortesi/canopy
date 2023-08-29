[![.github/workflows/ci.yml](https://github.com/cortesi/canopy/actions/workflows/ci.yml/badge.svg)](https://github.com/cortesi/canopy/actions/workflows/ci.yml)


**Hey curious person - if you've stumbled onto this project, please know that Canopy is is not yet ready for human
consumption. I'll announce a release as soon as I feel it's worth anyone else's time.**

<center>
    <img width=350 src=".assets/shyness.jpg">
</center>


# Canopy: a terminal UI library for Rust

In a forest each tree spreads its branches wide to maximise access to sunlight, but also carefully avoids touching the
foliage of its neighbours. This phenomenon is called "crown shyness" - the forest canopy becomes an organic tiling of
the sky.

**Canopy** works just the same, but in your terminal. Interface elements are arranged in an ordered tree, with each node
managing only its children, who manage their own children in turn, until the leaf nodes tile the screen without overlap.
All interface operations are defined cleanly as traversals of this node tree.


# Docs

- [Manual](https://cortesi.github.io/canopy)
