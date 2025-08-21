use half::f16;

pub type Vec2f = nalgebra::Vector2<f32>;
pub type Vec3f = nalgebra::Vector3<f32>;
pub type Vec4f = nalgebra::Vector4<f32>;

pub type Vec2h = nalgebra::Vector2<f16>;
pub type Vec3h = nalgebra::Vector3<f16>;
pub type Vec4h = nalgebra::Vector4<f16>;

pub type Vec2i = nalgebra::Vector2<i32>;
pub type Vec3i = nalgebra::Vector3<i32>;
pub type Vec4i = nalgebra::Vector4<i32>;

pub type Vec2u = nalgebra::Vector2<u32>;
pub type Vec3u = nalgebra::Vector3<u32>;
pub type Vec4u = nalgebra::Vector4<u32>;

pub type Mat4f = nalgebra::Matrix4<f32>;

pub async fn yield_async(timeout: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, timeout)
            .unwrap();
    });
    wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
}
