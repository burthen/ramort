use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoAnalyzePlan {
    pub package: Option<String>,
    pub features: Vec<String>,
    pub target: Option<String>,
    pub rustc_args: Vec<String>,
}

impl CargoAnalyzePlan {
    pub fn to_cargo_command(&self) -> Vec<String> {
        let mut cmd = vec![
            "cargo".into(),
            "check".into(),
            "--message-format=json".into(),
        ];
        if let Some(pkg) = &self.package {
            cmd.push("-p".into());
            cmd.push(pkg.clone());
        }
        if !self.features.is_empty() {
            cmd.push("--features".into());
            cmd.push(self.features.join(","));
        }
        if let Some(target) = &self.target {
            cmd.push("--target".into());
            cmd.push(target.clone());
        }
        cmd
    }
}
