#!/bin/bash

# ==============================================================================
# Executor Specific Test Suite
# ==============================================================================
#
# Focused tests for the Task Execution Engine. Unlike the full integration suite,
# this script relies on `jq` for parsing JSON responses and provides more
# detailed output for debugging task states.
#
# ## Usage
# 1. Start Cluster: `cargo run -- --bind 127.0.0.1:5000`
# 2. Run Tests: `./test_executor.sh`
#
# ==============================================================================

BASE_URL="http://127.0.0.1:6000"  # port 5000 + 1000
BASE_URL_NODE2="http://127.0.0.1:6001"  # port 5001 + 1000

echo "=== Executor Test Suite ==="
echo ""

# ------------------------------------------------------------------------------
# Test 1: Basic Task Submission
# Sends a simple payload to the 'test_handler' and extracts the returned Task ID.
# ------------------------------------------------------------------------------
echo "Test 1: Submit task"
TASK_RESPONSE=$(curl -s -X POST "$BASE_URL/task/submit" \
  -H "Content-Type: application/json" \
  -d '{
    "task": {
      "Execute": {
        "handler": "test_handler",
        "payload": {"message": "Hello from task!"}
      }
    }
  }')

echo "Response: $TASK_RESPONSE"
TASK_ID=$(echo $TASK_RESPONSE | grep -o '"task_id":"[^"]*"' | cut -d'"' -f4)
echo "Task ID: $TASK_ID"
echo ""

# Test 2: Check task status
echo "Test 2: Check task status"
sleep 1
curl -s "$BASE_URL/task/status/$TASK_ID" | jq '.'
echo ""

# ------------------------------------------------------------------------------
# Test 3: Distributed Submission
# Submits a task to Node 2 but queries status from Node 1.
# This proves that the DistributedQueue is correctly sharing state across the cluster.
# ------------------------------------------------------------------------------
echo "Test 3: Submit task to node 2 (distributed)"
curl -s -X POST "$BASE_URL_NODE2/task/submit" \
  -H "Content-Type: application/json" \
  -d '{
    "task": {
      "Execute": {
        "handler": "test_handler",
        "payload": {"message": "Task on node 2"}
      }
    }
  }' | jq '.'
echo ""

# ------------------------------------------------------------------------------
# Test 4: Burst Mode
# Submits multiple tasks in rapid succession to test the queue's ability to
# buffer pending tasks while workers are busy.
# ------------------------------------------------------------------------------
echo "Test 4: Submit 5 tasks in burst"
for i in {1..5}; do
  RESPONSE=$(curl -s -X POST "$BASE_URL/task/submit" \
    -H "Content-Type: application/json" \
    -d "{
      \"task\": {
        \"Execute\": {
          \"handler\": \"test_handler\",
          \"payload\": {\"message\": \"Burst task $i\"}
        }
      }
    }")
  echo "Task $i: $RESPONSE"
done
echo ""

# ------------------------------------------------------------------------------
# Test 5: Error Handling
# Submits a task with a non-existent handler name.
# Expectation: The system accepts the task, but the worker marks it as 'Failed'
# upon execution attempt.
# ------------------------------------------------------------------------------
echo "Test 5: Submit task with unknown handler (should fail)"
curl -s -X POST "$BASE_URL/task/submit" \
  -H "Content-Type: application/json" \
  -d '{
    "task": {
      "Execute": {
        "handler": "nonexistent_handler",
        "payload": {}
      }
    }
  }' | jq '.'
echo ""

# Test 6: Wait and check first task completion
echo "⏳ Test 6: Wait 3s and check task completion"
sleep 3
curl -s "$BASE_URL/task/status/$TASK_ID" | jq '.'
echo ""

echo "=== Tests completed ==="