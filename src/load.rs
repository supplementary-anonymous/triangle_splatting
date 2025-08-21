use futures::{AsyncBufReadExt, AsyncRead, AsyncReadExt, io::BufReader};
use half::f16;
use web_time::Instant;

use crate::{
    pbar::{Progress, ProgressBar},
    utils::{Vec2h, Vec3f, Vec3h, Vec4h},
};

const INVALID_FLIE: &str = "invalid model file";

pub struct TSplat {
    pub points: Vec<[Vec3f; 3]>,
    pub alpha_sigma: Vec<Vec2h>,
    pub sh: Vec<Vec4h>,
}

pub async fn read_tsplat<S: AsyncRead + Unpin>(
    byte_stream: S,
    pbar: ProgressBar,
) -> Result<TSplat, String> {
    pbar.update_status("downloading model".to_string()).await;

    let mut reader = BufReader::with_capacity(1 << 20, byte_stream);

    let mut current_line = String::new();
    reader
        .read_line(&mut current_line)
        .await
        .map_err(|_| INVALID_FLIE)?;

    if current_line.trim() != "TSPLAT" {
        return Err(INVALID_FLIE.into());
    }

    let mut num_tris_bytes = [0u8; 4];
    reader
        .read_exact(&mut num_tris_bytes)
        .await
        .map_err(|_| INVALID_FLIE)?;
    let num_tris = u32::from_le_bytes(num_tris_bytes) as usize;

    web_sys::console::log_1(&format!("num_tris: {}", num_tris).into());

    let points_bytes = num_tris * 3 * std::mem::size_of::<Vec3f>();
    let alpha_sigma_bytes = num_tris * std::mem::size_of::<Vec2h>();
    // let sh_bytes = num_tris * 12 * std::mem::size_of::<Vec4h>();
    let sh_bytes = num_tris * std::mem::size_of::<Vec3h>();
    let expected_bytes = points_bytes + alpha_sigma_bytes + sh_bytes;

    let mut buffer = vec![0u8; expected_bytes];
    let mut bytes_read = 0;
    let mut last_update_time = Instant::now();

    while bytes_read < expected_bytes {
        let read = reader
            .read(&mut buffer[bytes_read..])
            .await
            .map_err(|_| INVALID_FLIE)?;
        if read == 0 {
            return Err(INVALID_FLIE.into());
        }
        bytes_read += read;

        let now = Instant::now();
        if now.duration_since(last_update_time).as_millis() > 20 {
            let progress = bytes_read as f32 / expected_bytes as f32;
            pbar.update_progress(0.6 * progress).await;
            last_update_time = now;
        }
    }

    pbar.update_status("parsing file".to_string()).await;

    let mut bytes_parsed = 0;

    let points: Vec<[Vec3f; 3]> =
        bytemuck::cast_slice(&buffer[bytes_parsed..bytes_parsed + points_bytes]).to_vec();
    bytes_parsed += points_bytes;

    let alpha_sigma: Vec<Vec2h> =
        bytemuck::cast_slice(&buffer[bytes_parsed..bytes_parsed + alpha_sigma_bytes]).to_vec();
    bytes_parsed += alpha_sigma_bytes;

    // let sh: Vec<Vec4h> =
    //     bytemuck::cast_slice(&buffer[bytes_parsed..bytes_parsed + sh_bytes]).to_vec();

    // To fit within github limits, we only load DC terms for SH and set others to zero.
    let dc: Vec<Vec3h> =
        bytemuck::cast_slice(&buffer[bytes_parsed..bytes_parsed + sh_bytes]).to_vec();
    let sh = dc
        .into_iter()
        .map(|v| Vec4h::new(v.x, v.y, v.z, f16::from_f32(0.0)))
        .chain(std::iter::repeat(Vec4h::new(
            f16::from_f32(0.0),
            f16::from_f32(0.0),
            f16::from_f32(0.0),
            f16::from_f32(0.0),
        )))
        .take(num_tris * 12)
        .collect::<Vec<_>>();

    pbar.update_status("done parsing".to_string()).await;

    let forward = Vec3f::new(0.8644, 0.4385, 0.2458);
    let mut kv: Vec<(f32, usize)> = points
        .iter()
        .map(|tri| {
            let c = tri[0] + tri[1] + tri[2] / 3.0;
            c.dot(&forward)
        })
        .zip(0..)
        .collect();
    kv.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

    let points_sorted = kv.iter().map(|(_, idx)| points[*idx]).collect::<Vec<_>>();
    let alpha_sigma_sorted = kv
        .iter()
        .map(|(_, idx)| alpha_sigma[*idx])
        .collect::<Vec<_>>();
    let sh_sorted = kv.iter().map(|(_, idx)| sh[*idx]).collect::<Vec<_>>();

    Ok(TSplat {
        points: points_sorted,
        alpha_sigma: alpha_sigma_sorted,
        sh: sh_sorted,
    })
}
