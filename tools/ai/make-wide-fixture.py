# Writes src-tauri/src/ai/testdata/wide.onnx: a tiny model with a 12-way Split and a 20-input Concat,
# for the test of the wide-node rewrite (onnx_wide.rs). Needs the `onnx` Python package.
import sys
import numpy as np
from onnx import helper, numpy_helper, TensorProto, checker, save

x = helper.make_tensor_value_info('x', TensorProto.FLOAT, [1, 24, 4])
y = helper.make_tensor_value_info('y', TensorProto.FLOAT, [1, 24, 20])
sizes = numpy_helper.from_array(np.array([2] * 12, dtype=np.int64), 'sizes')
parts = [f'p{i}' for i in range(12)]
nodes = [helper.make_node('Split', ['x', 'sizes'], parts, name='split', axis=1)]
# 12 parts, each scaled differently, then concatenated with 8 more copies along the last axis
scaled = []
for i, p in enumerate(parts):
    nodes.append(helper.make_node('Mul', [p, f'k{i}'], [f's{i}'], name=f'mul{i}'))
    scaled.append(f's{i}')
nodes.append(helper.make_node('Concat', scaled, ['joined'], name='join_parts', axis=1))  # back to [1,24,4]
nodes.append(helper.make_node('Concat', ['joined'] * 5, ['wide'], name='wide', axis=2))  # [1,24,20]
# a 20-input Concat
nodes.append(helper.make_node('Split', ['wide', 'cols'], [f'c{i}' for i in range(20)], name='cols_split', axis=2))
nodes.append(helper.make_node('Concat', [f'c{19 - i}' for i in range(20)], ['y'], name='reverse_cols', axis=2))
inits = [sizes, numpy_helper.from_array(np.array([1] * 20, dtype=np.int64), 'cols')] + [numpy_helper.from_array(np.array(i + 1, dtype=np.float32), f'k{i}') for i in range(12)]
g = helper.make_graph(nodes, 'wide', [x], [y], inits)
m = helper.make_model(g, opset_imports=[helper.make_opsetid('', 17)], ir_version=8)
checker.check_model(m)
save(m, sys.argv[1])
