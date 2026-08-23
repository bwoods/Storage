use clap::Args;
use log::{debug, trace};
use reedline_repl_rs::clap::{ArgAction, ArgMatches, FromArgMatches, Subcommand};
use std::fs::read;
use std::path::PathBuf;
use storage::File;
use ufbx::{LoadOpts, Mesh, MeshPart, Vec2, Vec3, VertexStream, generate_indices, load_memory};

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a table from a CSV file
    Import {
        /// Frame to create from the file.
        ///
        /// The supplied name is used as a prefix for the frames created from
        /// the file content.
        #[arg(required = true)]
        name: String,
        /// Path to a FBX file.
        #[arg(required = true)]
        path: PathBuf,

        #[command(flatten)]
        options: Options,
    },
}

pub fn verbs(
    args: ArgMatches,
    file: &mut File,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    Ok(Command::from_arg_matches(&args)?.run(file)?)
}

impl Command {
    pub(crate) fn run(self, file: &mut File) -> Result<Option<String>, Box<dyn std::error::Error>> {
        match self {
            Command::Import {
                name,
                path,
                options,
            } => import(file, path, name, options),
        }
    }
}

fn import(
    _file: &mut File,
    path: PathBuf,
    _name: String,
    options: Options,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let _scene = load_scene(read(&path)?, options)?;

    Ok(None)
}

fn load_scene(bytes: Vec<u8>, options: Options) -> Result<(Vec<Vertex>, Vec<u32>), Error> {
    let scene = load_memory(&bytes, options.into())?;

    let mut vertices = Vec::new();
    let mut triangles = 0;

    for node in &scene.nodes {
        if node.is_root {
            continue;
        }

        if let Some(mesh) = &node.mesh {
            let name = &node.element.name;
            triangles += mesh.num_triangles;

            trace!(
                "mesh: {} ({} triangles)",
                node.element.name, mesh.num_triangles
            );

            for part in &mesh.material_parts {
                let material = &mesh.materials[part.index as usize].element.name;
                trace!("material: {} ({} triangles)", material, part.num_triangles);

                vertices.append(&mut triangulate_mesh_part(mesh, part, &name, &material)?);
            }
        }

        if let Some(camera) = &node.camera {
            println!("{}", camera.field_of_view_deg);
        }
    }

    let stream = VertexStream::new(&mut vertices);
    let mut indices = Vec::new();
    indices.resize(triangles * 3, 0);

    // “This call will deduplicate vertices, modifying the arrays passed in `streams[]`,
    //  indices are written in `indices[]` and the number of unique vertices is returned.”
    //    — ufbx.github.io/elements/meshes/
    let len = generate_indices(&mut [stream], &mut indices, Default::default())?;
    vertices.truncate(len);

    Ok((vertices, indices))
}

fn triangulate_mesh_part(
    mesh: &Mesh,
    part: &MeshPart,
    name: &str,
    material: &str,
) -> Result<Vec<Vertex>, Error> {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    indices.resize(mesh.max_face_triangles * 3, 0);

    if !mesh.vertex_normal.exists {
        debug!("{} ▸ {} has no normals", name, material);
    }

    if !mesh.vertex_uv.exists {
        debug!("{} ▸ {} has no texcoords", name, material);
    }

    for &i in &part.face_indices {
        let face = mesh.faces[i as usize];
        let tris = mesh.triangulate_face(&mut indices, face) as usize;

        for &index in &indices[..tris * 3] {
            let n = index as usize;

            vertices.push(Vertex::new(
                mesh.vertex_position[n],
                match mesh.vertex_normal.exists {
                    true => Some(mesh.vertex_normal[n]),
                    false => None,
                },
                match mesh.vertex_uv.exists {
                    true => Some(mesh.vertex_uv[n]),
                    false => None,
                },
            ));
        }
    }

    Ok(vertices)
}
#[derive(Copy, Clone)]
struct Vertex {
    pub vp: [f32; 3],
    pub vn: Option<[f32; 3]>,
    pub vt: Option<[f32; 2]>,
}

impl Vertex {
    fn new(vp: Vec3, vn: Option<Vec3>, vt: Option<Vec2>) -> Self {
        let vp = [vp.x as f32, vp.y as f32, vp.z as f32];
        let vn = vn.map(|vn| [vn.x as f32, vn.y as f32, vn.z as f32]);
        let vt = vt.map(|vt| [vt.x as f32, vt.y as f32]);

        Self { vp, vn, vt }
    }
}

#[derive(Args, Copy, Clone, Debug)]
pub struct Options {
    /// Required to overwrite an existing frame.
    #[arg(long, action(ArgAction::SetTrue))]
    overwrite: bool,
    /// Do not import meshes.
    #[arg(long, action(ArgAction::SetTrue))]
    ignore_geometry: bool,
    /// Do not import lights.
    #[arg(long, action(ArgAction::SetTrue))]
    ignore_lights: bool,
    /// Do not import cameras.
    #[arg(long, action(ArgAction::SetTrue))]
    ignore_cameras: bool,
}

impl<'a> Into<LoadOpts<'a>> for Options {
    fn into(self) -> LoadOpts<'a> {
        let mut opts = LoadOpts::default();
        opts.ignore_geometry = self.ignore_geometry;
        // More to come…

        opts
    }
}

#[derive(Debug)]
struct Error(#[allow(unused)] ufbx::Error);

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as std::fmt::Debug>::fmt(self, f)
    }
}

impl From<ufbx::Error> for Error {
    fn from(error: ufbx::Error) -> Self {
        Error(error)
    }
}
