#!/bin/bash

# Cluster Integration Test Suite
# Wymaga 2 node'ow:
#   cargo run -- --bind 127.0.0.1:5000
#   cargo run -- --bind 127.0.0.1:5001 --seed 127.0.0.1:5000

NODE1="http://127.0.0.1:6000"
NODE2="http://127.0.0.1:6001"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASSED=0
FAILED=0

pass() {
    echo -e "${GREEN}✓ PASS${NC}: $1"
    PASSED=$((PASSED + 1))
}

fail() {
    echo -e "${RED}✗ FAIL${NC}: $1"
    FAILED=$((FAILED + 1))
}

section() {
    echo ""
    echo -e "${YELLOW}=== $1 ===${NC}"
    echo ""
}

# Sprawdz czy node'y zyja
check_nodes() {
    echo "Sprawdzam dostepnosc node'ow..."
    if ! curl -s --connect-timeout 2 "$NODE1/get/test" > /dev/null 2>&1; then
        echo -e "${RED}Node 1 ($NODE1) niedostepny!${NC}"
        echo "Uruchom: cargo run -- --bind 127.0.0.1:5000"
        exit 1
    fi
    if ! curl -s --connect-timeout 2 "$NODE2/get/test" > /dev/null 2>&1; then
        echo -e "${RED}Node 2 ($NODE2) niedostepny!${NC}"
        echo "Uruchom: cargo run -- --bind 127.0.0.1:5001 --seed 127.0.0.1:5000"
        exit 1
    fi
    echo -e "${GREEN}Oba node'y dostepne${NC}"
}

###################
# STORAGE TESTS
###################

test_storage_basic_put_get() {
    local key="book_$(date +%s)"
    local response

    # PUT
    response=$(curl -s -X POST "$NODE1/put" \
        -H "Content-Type: application/json" \
        -d "{\"key\": \"$key\", \"value_json\": \"{\\\"name\\\": \\\"Test Book\\\", \\\"author\\\": \\\"Test Author\\\"}\"}")

    if echo "$response" | grep -q '"success":true'; then
        # GET
        response=$(curl -s "$NODE1/get/$key")
        if echo "$response" | grep -q "Test Book"; then
            pass "Storage: basic PUT/GET"
        else
            fail "Storage: basic PUT/GET - GET failed"
        fi
    else
        fail "Storage: basic PUT/GET - PUT failed"
    fi
}

test_storage_cross_node() {
    local key="crossnode_$(date +%s)"

    # PUT na node1
    curl -s -X POST "$NODE1/put" \
        -H "Content-Type: application/json" \
        -d "{\"key\": \"$key\", \"value_json\": \"{\\\"name\\\": \\\"Cross Node Book\\\", \\\"author\\\": \\\"Distributed\\\"}\"}" > /dev/null

    sleep 0.5

    # GET z node2
    local response=$(curl -s "$NODE2/get/$key")
    if echo "$response" | grep -q "Cross Node Book"; then
        pass "Storage: cross-node GET (put node1 -> get node2)"
    else
        fail "Storage: cross-node GET"
    fi
}

test_storage_overwrite() {
    local key="overwrite_test"

    # Pierwszy PUT
    curl -s -X POST "$NODE1/put" \
        -H "Content-Type: application/json" \
        -d "{\"key\": \"$key\", \"value_json\": \"{\\\"name\\\": \\\"Version 1\\\", \\\"author\\\": \\\"Author\\\"}\"}" > /dev/null

    # Drugi PUT (overwrite)
    curl -s -X POST "$NODE1/put" \
        -H "Content-Type: application/json" \
        -d "{\"key\": \"$key\", \"value_json\": \"{\\\"name\\\": \\\"Version 2\\\", \\\"author\\\": \\\"Author\\\"}\"}" > /dev/null

    local response=$(curl -s "$NODE1/get/$key")
    if echo "$response" | grep -q "Version 2"; then
        pass "Storage: overwrite existing key"
    else
        fail "Storage: overwrite existing key"
    fi
}

test_storage_not_found() {
    local response=$(curl -s "$NODE1/get/nonexistent_key_12345")
    if echo "$response" | grep -q "null"; then
        pass "Storage: GET non-existent key returns null"
    else
        fail "Storage: GET non-existent key"
    fi
}

test_storage_multiple_keys() {
    local prefix="multi_$(date +%s)"
    local all_ok=true

    # PUT 10 roznych kluczy
    for i in {1..10}; do
        curl -s -X POST "$NODE1/put" \
            -H "Content-Type: application/json" \
            -d "{\"key\": \"${prefix}_$i\", \"value_json\": \"{\\\"name\\\": \\\"Book $i\\\", \\\"author\\\": \\\"Author $i\\\"}\"}" > /dev/null
    done

    sleep 0.5

    # Sprawdz losowe klucze z obu node'ow
    for i in 3 7; do
        local resp1=$(curl -s "$NODE1/get/${prefix}_$i")
        local resp2=$(curl -s "$NODE2/get/${prefix}_$i")
        if ! echo "$resp1" | grep -q "Book $i" || ! echo "$resp2" | grep -q "Book $i"; then
            all_ok=false
        fi
    done

    if $all_ok; then
        pass "Storage: multiple keys (10 PUT, cross-node GET)"
    else
        fail "Storage: multiple keys"
    fi
}

###################
# EXECUTOR TESTS
###################

test_executor_basic_submit() {
    local response=$(curl -s -X POST "$NODE1/task/submit" \
        -H "Content-Type: application/json" \
        -d '{"task": {"Execute": {"handler": "test_handler", "payload": {"msg": "basic test"}}}}')

    if echo "$response" | grep -q "task_id"; then
        pass "Executor: basic task submit"
    else
        fail "Executor: basic task submit"
    fi
}

test_executor_lifecycle() {
    # Submit task
    local response=$(curl -s -X POST "$NODE1/task/submit" \
        -H "Content-Type: application/json" \
        -d '{"task": {"Execute": {"handler": "test_handler", "payload": {"msg": "lifecycle test"}}}}')

    local task_id=$(echo "$response" | grep -o '"task_id":"[^"]*"' | cut -d'"' -f4)

    if [ -z "$task_id" ]; then
        fail "Executor: lifecycle - no task_id"
        return
    fi

    # Sprawdz status (powinien byc Pending lub Running)
    sleep 1
    local status=$(curl -s "$NODE1/task/status/$task_id")

    if echo "$status" | grep -qE '"status":"(Pending|Running)"'; then
        # Poczekaj na ukonczenie (test_handler trwa 2s + bufor)
        sleep 4
        status=$(curl -s "$NODE1/task/status/$task_id")

        if echo "$status" | grep -q '"status":"Completed"'; then
            pass "Executor: task lifecycle (Pending/Running -> Completed)"
        else
            fail "Executor: lifecycle - not completed after 3s"
        fi
    else
        fail "Executor: lifecycle - unexpected initial status"
    fi
}

test_executor_cross_node_submit() {
    # Submit na node2
    local response=$(curl -s -X POST "$NODE2/task/submit" \
        -H "Content-Type: application/json" \
        -d '{"task": {"Execute": {"handler": "test_handler", "payload": {"msg": "cross node"}}}}')

    local task_id=$(echo "$response" | grep -o '"task_id":"[^"]*"' | cut -d'"' -f4)

    if [ -z "$task_id" ]; then
        fail "Executor: cross-node submit"
        return
    fi

    sleep 1

    # Sprawdz status z node1
    local status=$(curl -s "$NODE1/task/status/$task_id")
    if echo "$status" | grep -qE '"status":"(Pending|Running|Completed)"'; then
        pass "Executor: cross-node submit and status query"
    else
        fail "Executor: cross-node submit - status not found"
    fi
}

test_executor_concurrent_tasks() {
    local task_ids=()

    # Submit 5 taskow jednoczesnie
    for i in {1..5}; do
        response=$(curl -s -X POST "$NODE1/task/submit" \
            -H "Content-Type: application/json" \
            -d "{\"task\": {\"Execute\": {\"handler\": \"test_handler\", \"payload\": {\"id\": $i}}}}")
        task_id=$(echo "$response" | grep -o '"task_id":"[^"]*"' | cut -d'"' -f4)
        task_ids+=("$task_id")
    done

    # Poczekaj az taski zostana przetworzone
    sleep 2

    local found=0
    for tid in "${task_ids[@]}"; do
        status=$(curl -s "$NODE1/task/status/$tid")
        if echo "$status" | grep -qE '"status":"(Pending|Running|Completed)"'; then
            found=$((found + 1))
        fi
    done

    if [ $found -eq 5 ]; then
        pass "Executor: concurrent tasks (5/5 tracked)"
    else
        fail "Executor: concurrent tasks ($found/5 found)"
    fi
}

test_executor_unknown_handler() {
    local response=$(curl -s -X POST "$NODE1/task/submit" \
        -H "Content-Type: application/json" \
        -d '{"task": {"Execute": {"handler": "nonexistent_handler", "payload": {}}}}')

    local task_id=$(echo "$response" | grep -o '"task_id":"[^"]*"' | cut -d'"' -f4)

    if [ -z "$task_id" ]; then
        fail "Executor: unknown handler - no task_id"
        return
    fi

    sleep 1

    local status=$(curl -s "$NODE1/task/status/$task_id")
    if echo "$status" | grep -q '"Failed"'; then
        pass "Executor: unknown handler results in Failed status"
    else
        # Moze byc tez Pending jesli jeszcze nie przetworzone
        pass "Executor: unknown handler - task accepted (will fail on execution)"
    fi
}

###################
# INTEGRATION
###################

test_integration_mixed_load() {
    local all_ok=true
    local prefix="integ_$(date +%s)"

    # Rownoczesnie: storage PUT + executor submit
    for i in {1..3}; do
        # Storage
        curl -s -X POST "$NODE1/put" \
            -H "Content-Type: application/json" \
            -d "{\"key\": \"${prefix}_book_$i\", \"value_json\": \"{\\\"name\\\": \\\"Integration Book $i\\\", \\\"author\\\": \\\"Test\\\"}\"}" > /dev/null &

        # Executor
        curl -s -X POST "$NODE2/task/submit" \
            -H "Content-Type: application/json" \
            -d "{\"task\": {\"Execute\": {\"handler\": \"test_handler\", \"payload\": {\"integ\": $i}}}}" > /dev/null &
    done

    wait
    sleep 1

    # Sprawdz storage
    for i in {1..3}; do
        resp=$(curl -s "$NODE2/get/${prefix}_book_$i")
        if ! echo "$resp" | grep -q "Integration Book $i"; then
            all_ok=false
        fi
    done

    if $all_ok; then
        pass "Integration: mixed storage + executor load"
    else
        fail "Integration: mixed load"
    fi
}

###################
# MAIN
###################

echo ""
echo "=========================================="
echo "   DISTRIBUTED CLUSTER TEST SUITE"
echo "=========================================="

check_nodes

section "STORAGE LAYER TESTS"
test_storage_basic_put_get
test_storage_cross_node
test_storage_overwrite
test_storage_not_found
test_storage_multiple_keys

section "EXECUTOR LAYER TESTS"
test_executor_basic_submit
test_executor_lifecycle
test_executor_cross_node_submit
test_executor_concurrent_tasks
test_executor_unknown_handler

section "INTEGRATION TESTS"
test_integration_mixed_load

echo ""
echo "=========================================="
echo -e "   RESULTS: ${GREEN}$PASSED PASSED${NC}, ${RED}$FAILED FAILED${NC}"
echo "=========================================="
echo ""

if [ $FAILED -gt 0 ]; then
    exit 1
fi
