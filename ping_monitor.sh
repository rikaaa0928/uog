 #!/bin/bash

TARGET_IP="192.168.9.1"
failure_count=0
max_failures=5

while true; do
  if ping -c 1 $TARGET_IP &> /dev/null; then
    echo "Ping to $TARGET_IP was successful."
    failure_count=0
  else
    echo "Ping to $TARGET_IP failed."
    ((failure_count++))
    echo "Consecutive failures: $failure_count"
  fi

  if [ $failure_count -ge $max_failures ]; then
    echo "Reached $max_failures consecutive failures. Restarting wg-quick@wg0..."
    sudo systemctl restart wg-quick@wg0
    if [ $? -eq 0 ]; then
        echo "Successfully restarted wg-quick@wg0."
    else
        echo "Failed to restart wg-quick@wg0. Please check permissions or the service name."
    fi
    echo "Waiting for 5 seconds..."
    failure_count=0
    sleep 5
  fi

  sleep 1
done
