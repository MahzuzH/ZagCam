use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;

pub const IN_W: usize = 256;
pub const IN_H: usize = 144;

pub struct Segmenter {
    session: Session,
    input_name: String,
    output_name: String,
}

impl Segmenter {
    pub fn load(path: &Path) -> Result<Self, String> {
        let env = ort::init().build().map_err(|why| format!("onnxruntime init: {why}"))?;
        let builder =
            Session::builder(&env).map_err(|why| format!("onnxruntime session: {why}"))?;
        let mut builder = builder
            .with_intra_threads(1)
            .map_err(|why| format!("onnxruntime threads: {why}"))?;
        let session = builder
            .commit_from_file(path)
            .map_err(|why| format!("cannot load {}: {why}", path.display()))?;
        let input_name = session.inputs()[0].name().to_string();
        let output_name = session.outputs()[0].name().to_string();
        Ok(Self { session, input_name, output_name })
    }

    /// `input` is RGB f32 in 0..1, length `IN_W * IN_H * 3`.
    /// `mask` is the person probability in 0..1, length `IN_W * IN_H`.
    pub fn infer(&mut self, input: &[f32], mask: &mut [f32]) -> Result<(), String> {
        let tensor = Tensor::from_array(([1usize, 3, IN_H, IN_W], input.to_vec()))
            .map_err(|why| format!("input tensor: {why}"))?;
        let outputs = self
            .session
            .run(ort::inputs![self.input_name.as_str() => tensor])
            .map_err(|why| format!("inference failed: {why}"))?;
        let (_, data) = outputs[self.output_name.as_str()]
            .try_extract_tensor::<f32>()
            .map_err(|why| format!("mask output: {why}"))?;
        if data.len() < mask.len() {
            return Err(format!("mask output too small: {} < {}", data.len(), mask.len()));
        }
        mask.copy_from_slice(&data[..mask.len()]);
        Ok(())
    }
}
