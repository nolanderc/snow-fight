#!/bin/bash

shopt -s extglob

for shader in *.+(vert|frag)
do
    glslangValidator $shader -V -o "$shader.spv"
done


