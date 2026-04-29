use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

#[allow(dead_code)]
pub struct LayeredStageFixture {
    pub dir: PathBuf,
    pub root: PathBuf,
    pub working: PathBuf,
    pub asset: PathBuf,
}

impl LayeredStageFixture {
    pub fn new(name: &str) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("bif_c4a_{name}_{}_{}", std::process::id(), id));
        std::fs::create_dir_all(&dir).expect("create temp fixture dir");
        let root = dir.join("root.usda");
        let working = dir.join("working.usda");
        let asset = dir.join("asset.usda");

        write(
            &working,
            r#"#usda 1.0

"#,
        );
        write(
            &asset,
            r#"#usda 1.0

def Xform "World"
{
    def Xform "Cube"
    {
    }
}

def Scope "Materials"
{
    def Material "Red"
    {
        token outputs:surface.connect = </Materials/Red/PreviewSurface.outputs:surface>

        def Shader "PreviewSurface"
        {
            uniform token info:id = "UsdPreviewSurface"
            float inputs:roughness = 0.2
            color3f inputs:diffuseColor = (1, 0, 0)
            token outputs:surface
        }
    }
}

def Xform "Model" (
    variants = {
        string lod = "low"
    }
    prepend variantSets = "lod"
)
{
    variantSet "lod" = {
        "low" {
            def Xform "Low"
            {
            }
        }
        "high" {
            def Xform "High"
            {
            }
        }
    }
}

"#,
        );
        write(
            &root,
            r#"#usda 1.0
(
    subLayers = [
        @working.usda@,
        @asset.usda@
    ]
)

"#,
        );

        Self {
            dir,
            root,
            working,
            asset,
        }
    }

    #[allow(dead_code)]
    pub fn working_text(&self) -> String {
        std::fs::read_to_string(&self.working).expect("read working layer")
    }
}

impl Drop for LayeredStageFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub fn write(path: &Path, text: &str) {
    std::fs::write(path, text).expect("write usda fixture");
}

pub fn working_layer_id(stage: &bif_core::usd::UsdStage) -> String {
    stage
        .get_layer_stack()
        .expect("layer stack")
        .layers
        .into_iter()
        .find(|l| l.identifier.ends_with("working.usda"))
        .map(|l| l.identifier)
        .expect("working.usda layer")
}
