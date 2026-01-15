for id in $(seq 1 100); do
    echo "ingest $id"
    curl -s -X POST "http://192.168.1.133:6000/ingest/$id" >/dev/null
    sleep 0.2
  done
