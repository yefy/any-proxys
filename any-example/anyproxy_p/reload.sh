#!/bin/bash
for i in $(seq 1 100); do
    echo $i
    ./anyproxy --sigfile reload
    echo $i
    sleep 10
done
