#!/bin/bash

# ==============================================================================
# Bulk Ingestion Test Script
# ==============================================================================
#
# A utility script to populate the cluster with data by sequentially ingesting
# 100 books from Project Gutenberg.
#
# ## Purpose
# - Populates the Datalake and Inverted Index with real data.
# - Generates a steady stream of background traffic for testing replication.
#
# ## Usage
# Adjust the NODE_URL variable to match your target node's IP and port.
# ==============================================================================

# Target cluster node (Primary entry point)
NODE_URL="http://192.168.1.133:6000"

echo "Starting bulk ingestion of 100 books to $NODE_URL..."

# Loop through Gutenberg Book IDs 1 to 100
for id in $(seq 1 100); do
    echo "Triggering ingestion for Book ID: $id"

    # Send POST request to the ingestion endpoint
    # -s: Silent mode (suppress progress bar)
    # Output is discarded (> /dev/null) to keep the terminal output clean
    curl -s -X POST "$NODE_URL/ingest/$id" >/dev/null

    # Rate limiting: Sleep 200ms between requests.
    # This prevents overwhelming the local node's download worker and 
    # respects upstream Project Gutenberg rate limits.
    sleep 0.2
done

echo "Bulk ingestion complete."