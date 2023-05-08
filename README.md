# Introduction

A super simple command line utility to create a Nightcore edit of any audio file

## Nightcore

There's some Spotify account called "sped up nightcore" that literally just posts versions of 
popular songs sped up ~35%. That increase in tempo induces an effect similar to a change in pitch. 
More info can be found on the [Wikipedia page](https://en.wikipedia.org/wiki/Nightcore).
This is just an attempt to make a super simple and fast program to do the same thing.

## Current State

Using the `dsp` Rust crate to perform resampling. Currently takes ~18s to resample a 30s track (not great).