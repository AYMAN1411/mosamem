//! Rewrites an ONNX model so that no Split or Concat node touches more tensors than the GPU path
//! allows per shader (WebGPU guarantees 8 storage buffers per stage and Apple GPUs give 10).
//! A Split with many outputs becomes a Split into groups followed by a Split per group, and a
//! Concat with many inputs becomes a tree of Concats. The results are bit-for-bit the same.
//!
//! The model is edited at the protobuf wire level: only the graph's node list changes and a few
//! small initializers are added; every other byte is copied as it is. Field numbers are from the
//! public ONNX specification (onnx/onnx.proto).

use std::collections::HashMap;

/// Most outputs of a Split / inputs of a Concat after the rewrite. With the data tensor (and a
/// Split's sizes input) a node touches at most 10 tensors.
pub const LIMIT: usize = 8;

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "ONNX rewrite: {}", self.0)
    }
}

type Res<T> = Result<T, Error>;

// ---- wire format ----

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

enum Field<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Reader { b, at: 0 }
    }
    fn varint(&mut self) -> Res<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = *self.b.get(self.at).ok_or_else(|| Error("truncated varint".into()))?;
            self.at += 1;
            v |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(Error("varint too long".into()))
    }
    /// The next field: (number, value, its raw bytes including the key).
    fn next(&mut self) -> Res<Option<(u32, Field<'a>, &'a [u8])>> {
        if self.at >= self.b.len() {
            return Ok(None);
        }
        let start = self.at;
        let key = self.varint()?;
        let (num, wire) = ((key >> 3) as u32, key & 7);
        let f = match wire {
            0 => Field::Varint(self.varint()?),
            1 => {
                self.at += 8;
                Field::Fixed
            }
            2 => {
                let n = self.varint()? as usize;
                let end = self.at.checked_add(n).filter(|e| *e <= self.b.len()).ok_or_else(|| Error("truncated field".into()))?;
                let s = &self.b[self.at..end];
                self.at = end;
                Field::Bytes(s)
            }
            5 => {
                self.at += 4;
                Field::Fixed
            }
            w => return Err(Error(format!("wire type {w}"))),
        };
        if self.at > self.b.len() {
            return Err(Error("truncated field".into()));
        }
        Ok(Some((num, f, &self.b[start..self.at])))
    }
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}
fn put_bytes(out: &mut Vec<u8>, num: u32, b: &[u8]) {
    put_varint(out, ((num as u64) << 3) | 2);
    put_varint(out, b.len() as u64);
    out.extend_from_slice(b);
}
fn put_int(out: &mut Vec<u8>, num: u32, v: i64) {
    put_varint(out, (num as u64) << 3);
    put_varint(out, v as u64);
}

fn utf8(b: &[u8]) -> Res<String> {
    String::from_utf8(b.to_vec()).map_err(|_| Error("bad string".into()))
}

// ---- the parts of ONNX messages we need ----

/// NodeProto: input 1, output 2, name 3, op_type 4, attribute 5.
struct Node {
    inputs: Vec<String>,
    outputs: Vec<String>,
    name: String,
    op: String,
    /// AttributeProto name → its int value (field 3), for `axis`.
    ints: HashMap<String, i64>,
    /// `value` tensor of a Constant node.
    value: Option<Vec<i64>>,
}

fn parse_node(b: &[u8]) -> Res<Node> {
    let mut n = Node { inputs: vec![], outputs: vec![], name: String::new(), op: String::new(), ints: HashMap::new(), value: None };
    let mut r = Reader::new(b);
    while let Some((num, f, _)) = r.next()? {
        match (num, f) {
            (1, Field::Bytes(s)) => n.inputs.push(utf8(s)?),
            (2, Field::Bytes(s)) => n.outputs.push(utf8(s)?),
            (3, Field::Bytes(s)) => n.name = utf8(s)?,
            (4, Field::Bytes(s)) => n.op = utf8(s)?,
            (5, Field::Bytes(s)) => {
                // AttributeProto: name 1, i 3, t 5
                let (mut name, mut int, mut tensor) = (String::new(), None, None);
                let mut a = Reader::new(s);
                while let Some((an, af, _)) = a.next()? {
                    match (an, af) {
                        (1, Field::Bytes(v)) => name = utf8(v)?,
                        (3, Field::Varint(v)) => int = Some(v as i64),
                        (5, Field::Bytes(v)) => tensor = Some(v),
                        _ => {}
                    }
                }
                if let Some(i) = int {
                    n.ints.insert(name.clone(), i);
                }
                if name == "value" {
                    if let Some(t) = tensor {
                        n.value = tensor_int64(t)?.map(|(_, v)| v);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(n)
}

/// TensorProto (data_type 2, int64_data 7, name 8, raw_data 9): its name and values, if INT64.
fn tensor_int64(b: &[u8]) -> Res<Option<(String, Vec<i64>)>> {
    let (mut name, mut ty, mut vals, mut raw) = (String::new(), 0, vec![], None);
    let mut r = Reader::new(b);
    while let Some((num, f, _)) = r.next()? {
        match (num, f) {
            (2, Field::Varint(v)) => ty = v,
            (7, Field::Varint(v)) => vals.push(v as i64),
            (7, Field::Bytes(p)) => {
                let mut pr = Reader::new(p);
                while pr.at < p.len() {
                    vals.push(pr.varint()? as i64);
                }
            }
            (8, Field::Bytes(s)) => name = utf8(s)?,
            (9, Field::Bytes(s)) => raw = Some(s),
            _ => {}
        }
    }
    if ty != 7 {
        return Ok(None);
    }
    if let Some(raw) = raw {
        vals = raw.chunks_exact(8).map(|c| i64::from_le_bytes(c.try_into().unwrap())).collect();
    }
    Ok(Some((name, vals)))
}

fn encode_node(op: &str, name: &str, inputs: &[String], outputs: &[String], axis: i64) -> Vec<u8> {
    let mut n = vec![];
    for i in inputs {
        put_bytes(&mut n, 1, i.as_bytes());
    }
    for o in outputs {
        put_bytes(&mut n, 2, o.as_bytes());
    }
    put_bytes(&mut n, 3, name.as_bytes());
    put_bytes(&mut n, 4, op.as_bytes());
    if op != "Identity" {
        let mut a = vec![];
        put_bytes(&mut a, 1, b"axis");
        put_int(&mut a, 3, axis);
        put_int(&mut a, 20, 2); // AttributeType INT
        put_bytes(&mut n, 5, &a);
    }
    n
}

fn encode_int64_tensor(name: &str, v: &[i64]) -> Vec<u8> {
    let mut t = vec![];
    put_int(&mut t, 1, v.len() as i64); // dims
    put_int(&mut t, 2, 7); // INT64
    put_bytes(&mut t, 8, name.as_bytes());
    let raw: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
    put_bytes(&mut t, 9, &raw);
    t
}

// ---- the rewrite ----

struct Rewriter<'a> {
    nodes: Vec<u8>,
    inits: Vec<u8>,
    known: &'a HashMap<String, Vec<i64>>,
    uid: usize,
    changed: usize,
}

impl Rewriter<'_> {
    fn fresh(&mut self, base: &str) -> String {
        self.uid += 1;
        format!("{base}__lawha{}", self.uid)
    }
    fn sizes(&mut self, base: &str, v: &[i64]) -> String {
        let name = self.fresh(base);
        put_bytes(&mut self.inits, 5, &encode_int64_tensor(&name, v));
        name
    }
    fn emit(&mut self, op: &str, base: &str, inputs: &[String], outputs: &[String], axis: i64) {
        let name = self.fresh(base);
        put_bytes(&mut self.nodes, 1, &encode_node(op, &name, inputs, outputs, axis));
    }

    /// Writes the node's replacement, or false to keep it as it is.
    fn node(&mut self, n: &Node) -> bool {
        let axis = n.ints.get("axis").copied().unwrap_or(0);
        if n.op == "Split" && n.outputs.len() > LIMIT && n.inputs.len() == 2 {
            let Some(sizes) = self.known.get(&n.inputs[1]).cloned() else { return false };
            if sizes.len() != n.outputs.len() {
                return false;
            }
            let groups: Vec<Vec<usize>> = (0..sizes.len()).collect::<Vec<_>>().chunks(LIMIT).map(|c| c.to_vec()).collect();
            if groups.len() > LIMIT {
                return false; // would need another level; none of our models has that many
            }
            let mids: Vec<String> = groups.iter().map(|_| self.fresh(&n.name)).collect();
            let gs: Vec<i64> = groups.iter().map(|g| g.iter().map(|j| sizes[*j]).sum()).collect();
            let gsn = self.sizes(&n.name, &gs);
            self.emit("Split", &n.name, &[n.inputs[0].clone(), gsn], &mids, axis);
            for (mid, g) in mids.iter().zip(&groups) {
                let outs: Vec<String> = g.iter().map(|j| n.outputs[*j].clone()).collect();
                if g.len() == 1 {
                    self.emit("Identity", &n.name, std::slice::from_ref(mid), &outs, 0);
                } else {
                    let sn = self.sizes(&n.name, &g.iter().map(|j| sizes[*j]).collect::<Vec<_>>());
                    self.emit("Split", &n.name, &[mid.clone(), sn], &outs, axis);
                }
            }
            self.changed += 1;
            return true;
        }
        if n.op == "Concat" && n.inputs.len() > LIMIT {
            let mut layer = n.inputs.clone();
            while layer.len() > LIMIT {
                let mut next = vec![];
                for chunk in layer.chunks(LIMIT) {
                    if chunk.len() == 1 {
                        next.push(chunk[0].clone());
                        continue;
                    }
                    let o = self.fresh(&n.name);
                    self.emit("Concat", &n.name, chunk, std::slice::from_ref(&o), axis);
                    next.push(o);
                }
                layer = next;
            }
            self.emit("Concat", &n.name, &layer, &n.outputs, axis);
            self.changed += 1;
            return true;
        }
        false
    }
}

fn rewrite_graph(g: &[u8]) -> Res<(Vec<u8>, usize)> {
    // first pass: int64 initializers and constants (the split sizes)
    let mut known: HashMap<String, Vec<i64>> = HashMap::new();
    let mut r = Reader::new(g);
    while let Some((num, f, _)) = r.next()? {
        match (num, f) {
            (5, Field::Bytes(t)) => {
                if let Some((name, v)) = tensor_int64(t)? {
                    known.insert(name, v);
                }
            }
            (1, Field::Bytes(nb)) => {
                let n = parse_node(nb)?;
                if n.op == "Constant" {
                    if let (Some(v), Some(o)) = (n.value, n.outputs.first()) {
                        known.insert(o.clone(), v);
                    }
                }
            }
            _ => {}
        }
    }
    let mut w = Rewriter { nodes: vec![], inits: vec![], known: &known, uid: 0, changed: 0 };
    let mut rest = vec![];
    let mut r = Reader::new(g);
    while let Some((num, f, raw)) = r.next()? {
        match (num, f) {
            (1, Field::Bytes(nb)) => {
                let n = parse_node(nb)?;
                if !w.node(&n) {
                    w.nodes.extend_from_slice(raw);
                }
            }
            _ => rest.extend_from_slice(raw),
        }
    }
    let changed = w.changed;
    // nodes first (their order is the graph's order), then everything else, then the new sizes
    let mut out = w.nodes;
    out.extend_from_slice(&rest);
    out.extend_from_slice(&w.inits);
    Ok((out, changed))
}

/// The model with its wide Split and Concat nodes rewritten, and how many nodes changed.
pub fn rewrite(model: &[u8]) -> Res<(Vec<u8>, usize)> {
    let mut out = Vec::with_capacity(model.len() + 4096);
    let mut changed = 0;
    let mut graphs = 0;
    let mut r = Reader::new(model);
    while let Some((num, f, raw)) = r.next()? {
        match (num, f) {
            (7, Field::Bytes(g)) => {
                let (ng, c) = rewrite_graph(g)?;
                put_bytes(&mut out, 7, &ng);
                changed += c;
                graphs += 1;
            }
            _ => out.extend_from_slice(raw),
        }
    }
    if graphs != 1 {
        return Err(Error("not an ONNX model".into()));
    }
    Ok((out, changed))
}

/// The most tensors any node of the main graph touches (for tests and checks).
pub fn widest_node(model: &[u8]) -> Res<usize> {
    let mut widest = 0;
    let mut r = Reader::new(model);
    while let Some((num, f, _)) = r.next()? {
        if let (7, Field::Bytes(g)) = (num, f) {
            let mut gr = Reader::new(g);
            while let Some((n, gf, _)) = gr.next()? {
                if let (1, Field::Bytes(nb)) = (n, gf) {
                    let node = parse_node(nb)?;
                    widest = widest.max(node.inputs.len() + node.outputs.len());
                }
            }
        }
    }
    Ok(widest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_round_trip() {
        for v in [0u64, 1, 127, 128, 300, u32::MAX as u64, (-1i64) as u64] {
            let mut b = vec![];
            put_varint(&mut b, v);
            assert_eq!(Reader::new(&b).varint().unwrap(), v);
        }
    }

    /// The fixture (tools/ai/make-wide-fixture.py) has a 12-way and a 20-way Split and a 20-input
    /// Concat. After the rewrite no node is that wide, and the model computes the same numbers.
    #[test]
    fn rewrite_keeps_the_results() {
        use ort::session::{builder::AutoDevicePolicy, Session};
        use ort::value::Tensor;
        let model = include_bytes!("testdata/wide.onnx");
        assert!(widest_node(model).unwrap() > 20);
        let (out, changed) = rewrite(model).unwrap();
        assert_eq!(changed, 4, "two Splits and two Concats");
        assert!(widest_node(&out).unwrap() <= LIMIT + 2);
        let env = ort::init().with_telemetry(false).build().unwrap();
        let x: Vec<f32> = (0..96).map(|i| (i as f32 * 0.37).sin()).collect();
        let mut results = vec![];
        for bytes in [model.as_slice(), out.as_slice()] {
            let mut s = Session::builder(&env).unwrap().with_auto_device(AutoDevicePolicy::PreferCPU).unwrap().commit_from_memory(bytes).unwrap();
            let o = s.run(ort::inputs![Tensor::from_array(([1usize, 24, 4], x.clone())).unwrap()]).unwrap();
            results.push(o[0].try_extract_tensor::<f32>().unwrap().1.to_vec());
        }
        assert_eq!(results[0].len(), 24 * 20);
        assert_eq!(results[0], results[1]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(rewrite(b"hello, not a model").is_err());
    }
}
