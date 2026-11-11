#!/bin/bash
# Logs zygotes and listener count every 200 ms until the master on :18699 is gone.
out=$1; : > $out; seen=0
while true; do
  m=$(ps -eo pid,args | grep "[p]am start artisan" | grep 18600 | grep -v systemd-run | awk '{print $1}' | head -1)
  if [ -z "$m" ]; then [ $seen = 1 ] && break; sleep 0.2; continue; fi; seen=1
  z=$(ps -eo pid,ppid,args | awk -v m=$m '$2==m && /__zygote/ {print $1}' | tr '\n' ' ')
  l=$(ss -ltnH 'sport = :18600' | wc -l)
  echo "$(date +%s.%N | cut -c1-14) zygotes=[$z] listeners=$l" >> $out
  sleep 0.2
done
