#!/bin/zsh
# Runs the GPU spike with N layers (GPU only) and prints the result.
source ~/.cargo/env
cd "${0:A:h}/.."
n=$1
rm -f spikes/out/gpu-${n}layers.json spikes/out/error.json
VITE_SPIKE=1 VITE_SPIKE_GPU_ONLY=1 VITE_SPIKE_LAYERS=$n npx tauri dev > spikes/out/run-$n.log 2>&1 &
for i in {1..120}; do
  [[ -f spikes/out/gpu-${n}layers.json || -f spikes/out/error.json ]] && break
  grep -q "Error" spikes/out/run-$n.log && break
  sleep 3
done
sleep 3; pkill -f "target/debug/lawha"; pkill -f "node.*vite"; sleep 1
cat spikes/out/error.json 2>/dev/null
python3 -c "
import json;d=json.load(open('spikes/out/gpu-${n}layers.json'))
print('$n layers'); [print(' ',k, v) for k,v in d.items() if k not in ('canvas','atlasSlots','refreshIntervalMs')]"
