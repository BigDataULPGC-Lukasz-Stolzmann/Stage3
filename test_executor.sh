#!/bin/bash

# Executor Test Suite
# Uruchom klaster najpierw: cargo run -- --bind 127.0.0.1:5000
# Dodaj node: cargo run -- --bind 127.0.0.1:5001 --seed 127.0.0.1:5000

BASE_URL="http://127.0.0.1:6000"  # port 5000 + 1000
BASE_URL_NODE2="http://127.0.0.1:6001"  # port 5001 + 1000

echo "=== Executor Test Suite ==="
echo ""

# Test 1: Submit task (podstawowy)
echo "📝 Test 1: Submit task"
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
echo "📊 Test 2: Check task status"
sleep 1
curl -s "$BASE_URL/task/status/$TASK_ID" | jq '.'
echo ""

# Test 3: Submit task do drugiego node'a (distributed)
echo "🌐 Test 3: Submit task to node 2 (distributed)"
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

# Test 4: Submit multiple tasks (burst)
echo "🚀 Test 4: Submit 5 tasks in burst"
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

# Test 5: Unknown handler (error case)
echo "❌ Test 5: Submit task with unknown handler (should fail)"
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
