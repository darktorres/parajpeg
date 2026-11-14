// C entry points for the Rust crate (src/lib.rs): parajpeg's C++ API behind two functions, so no C++ exception and no
// C++ type crosses into Rust. Both return 0 on success and a non-zero code otherwise.
#include <parajpeg/parajpeg.h>

#include <cstddef>
#include <cstdint>
#include <mutex>

// parajpeg's CPU decoder shares one thread pool (decoder.cpp's shared_pool), and the pool runs one parallel job at a
// time: its single job slot is overwritten by a second caller, whose workers then never finish the first's job. Two
// decodes on several threads at once therefore hang. The decodes are serialized here; the header read doesn't use
// the pool, so it needs no lock.
static std::mutex decode_lock;

extern "C" {

// Headers only: the image's size, and its component count (3 for the YCbCr or RGB images the caller can decode).
int parajpeg_shim_header(const uint8_t* data, size_t size, uint32_t* width, uint32_t* height, int* components) {
    try {
        parajpeg::ImageInfo info;
        if (!parajpeg::read_header(data, size, info)) return 1;
        *width = info.width;
        *height = info.height;
        *components = info.components;
        return 0;
    } catch (...) {
        return 2;
    }
}

// The whole image as packed RGB, on the CPU backend with `threads` threads (0: the hardware's count). `stride` is the
// bytes per output row, at least width * 3, and the output must hold height rows of it.
int parajpeg_shim_decode_rgb(const uint8_t* data, size_t size, int threads, uint8_t* dst, size_t stride) {
    try {
        parajpeg::DecodeOptions options;
        options.format = parajpeg::PixelFormat::RGB;
        options.backend = parajpeg::Backend::Cpu;
        options.threads = threads;
        std::lock_guard<std::mutex> lock(decode_lock);
        return parajpeg::decode_into(data, size, options, dst, stride) ? 0 : 1;
    } catch (...) {
        return 2;
    }
}

}  // extern "C"
