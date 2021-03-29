#!/bin/bash

curl \
    -v \
    --header "Authorization: Bearer ${TOKEN}" \
    http://localhost:3030/api/notebooks/7-jUf4icSeygRCsZVjuA1A \
| jq .
