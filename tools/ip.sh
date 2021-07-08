#!/usr/bin/env bash
ip addr show eth0 | awk '$1 == "inet" {gsub(/\/.*$/, "", $2); print $2}'
