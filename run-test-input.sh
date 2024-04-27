#!/usr/bin/env bash
rm pom
(sleep 1s && ./test-input | ydotool type -f -) &
cargo r -- --db-path ./pom
