#!/bin/bash
curl \
    -v \
    --header "Content-Type: application/json" \
    --header "Authorization: Bearer ${TOKEN}" \
    --request POST \
    --data @notebook.json \
    http://localhost:3030/api/notebooks
