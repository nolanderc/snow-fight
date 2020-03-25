use image::RgbaImage;

pub fn from_image(
    image: &RgbaImage,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
) -> wgpu::TextureView {
    let size = wgpu::Extent3d {
        width: image.width(),
        height: image.height(),
        depth: 1,
    };

    let texture_desc = wgpu::TextureDescriptor {
        size,
        array_layer_count: 1,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsage::READ_ALL | wgpu::TextureUsage::COPY_DST,
    };

    let texture = device.create_texture(&texture_desc);

    let row_size = (4 * image.width() + 255) / 256 * 256;
    let mut bytes = Vec::with_capacity(4 * (row_size * image.height()) as usize);

    for row in 0..image.height() {
        for col in 0..image.width() {
            let color = image.get_pixel(col, row);
            bytes.extend_from_slice(&color.0)
        }

        // Each row must be a multiple of 256 bytes when we copy to the texture
        while bytes.len() < ((row + 1) * row_size) as usize {
            bytes.push(127);
        }
    }

    let texel_buffer = device
        .create_buffer_mapped(bytes.len(), wgpu::BufferUsage::COPY_SRC)
        .fill_from_slice(&bytes);

    let source_view = wgpu::BufferCopyView {
        buffer: &texel_buffer,
        offset: 0,
        row_pitch: row_size,
        image_height: image.height(),
    };

    let dest_view = wgpu::TextureCopyView {
        texture: &texture,
        mip_level: 0,
        array_layer: 0,
        origin: wgpu::Origin3d::ZERO,
    };

    encoder.copy_buffer_to_texture(source_view, dest_view, size);

    texture.create_default_view()
}
