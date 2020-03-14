#!/bin/bash

shopt -s extglob

for shader in src/shaders/*.+(vert|frag)
do
    glslangValidator $shader -V -o "$shader.spv"
done


