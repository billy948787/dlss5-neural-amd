// What does promoting a D3D9 device to D3D9Ex actually cost?
//
// The x86 bridge takes its fast path by asking the game's device for a texture with a shared
// handle. That only works on a D3D9Ex device, so a plain-D3D9 game falls back to a CPU round trip.
// Promoting means intercepting device creation and handing the game an Ex device instead.
//
// The objection to that is D3DPOOL_MANAGED, which D3D9Ex is documented not to support. This
// measures the claim on this machine and this driver rather than trusting the documentation or my
// memory, and checks what the alternatives actually allow.

#include <d3d9.h>
#include <cstdio>

#pragma comment(lib, "d3d9.lib")

static const char *ok(HRESULT hr) { return SUCCEEDED(hr) ? "OK     " : "FAILED "; }

static void probe(IDirect3DDevice9 *dev, const char *what)
{
    std::printf("\n--- %s ---\n", what);

    IDirect3DTexture9 *t = nullptr;
    HRESULT hr;

    hr = dev->CreateTexture(256, 256, 1, 0, D3DFMT_A8R8G8B8, D3DPOOL_MANAGED, &t, nullptr);
    std::printf("  %s D3DPOOL_MANAGED texture            hr=0x%08lX\n", ok(hr), hr);
    if (t) { t->Release(); t = nullptr; }

    hr = dev->CreateTexture(256, 256, 1, 0, D3DFMT_A8R8G8B8, D3DPOOL_DEFAULT, &t, nullptr);
    std::printf("  %s D3DPOOL_DEFAULT texture            hr=0x%08lX\n", ok(hr), hr);
    if (t)
    {
        D3DLOCKED_RECT r {};
        HRESULT lock = t->LockRect(0, &r, nullptr, 0);
        std::printf("  %s   LockRect on it                   hr=0x%08lX\n", ok(lock), lock);
        if (SUCCEEDED(lock)) t->UnlockRect(0);
        t->Release();
        t = nullptr;
    }

    hr = dev->CreateTexture(256, 256, 1, D3DUSAGE_DYNAMIC, D3DFMT_A8R8G8B8, D3DPOOL_DEFAULT, &t,
        nullptr);
    std::printf("  %s D3DPOOL_DEFAULT + DYNAMIC          hr=0x%08lX\n", ok(hr), hr);
    if (t)
    {
        D3DLOCKED_RECT r {};
        HRESULT lock = t->LockRect(0, &r, nullptr, 0);
        std::printf("  %s   LockRect on it                   hr=0x%08lX\n", ok(lock), lock);
        if (SUCCEEDED(lock)) t->UnlockRect(0);
        t->Release();
        t = nullptr;
    }

    // The one the bridge actually needs.
    HANDLE shared = nullptr;
    hr = dev->CreateTexture(256, 256, 1, D3DUSAGE_RENDERTARGET, D3DFMT_A8R8G8B8, D3DPOOL_DEFAULT,
        &t, &shared);
    std::printf("  %s RENDERTARGET + shared handle       hr=0x%08lX handle=%p\n", ok(hr), hr,
        shared);
    if (t) t->Release();

    IDirect3DDevice9Ex *ex = nullptr;
    HRESULT q = dev->QueryInterface(IID_PPV_ARGS(&ex));
    std::printf("  %s QueryInterface IDirect3DDevice9Ex  hr=0x%08lX\n", ok(q), q);
    if (ex) ex->Release();
}

int main()
{
    HWND wnd = CreateWindowExW(0, L"STATIC", L"probe", WS_OVERLAPPED, 0, 0, 64, 64, nullptr,
        nullptr, nullptr, nullptr);
    if (wnd == nullptr) { std::printf("no window\n"); return 1; }

    D3DPRESENT_PARAMETERS pp {};
    pp.BackBufferWidth = 64;
    pp.BackBufferHeight = 64;
    pp.BackBufferFormat = D3DFMT_X8R8G8B8;
    pp.BackBufferCount = 1;
    pp.SwapEffect = D3DSWAPEFFECT_DISCARD;
    pp.hDeviceWindow = wnd;
    pp.Windowed = TRUE;

    // 1. Plain D3D9, which is what a legacy game creates.
    if (IDirect3D9 *d9 = Direct3DCreate9(D3D_SDK_VERSION))
    {
        IDirect3DDevice9 *dev = nullptr;
        HRESULT hr = d9->CreateDevice(D3DADAPTER_DEFAULT, D3DDEVTYPE_HAL, wnd,
            D3DCREATE_HARDWARE_VERTEXPROCESSING, &pp, &dev);
        std::printf("Direct3DCreate9 + CreateDevice      hr=0x%08lX\n", hr);
        if (SUCCEEDED(hr)) { probe(dev, "plain D3D9 device"); dev->Release(); }
        d9->Release();
    }

    // 2. D3D9Ex, which is what promotion would hand the game instead.
    IDirect3D9Ex *d9ex = nullptr;
    HRESULT hr = Direct3DCreate9Ex(D3D_SDK_VERSION, &d9ex);
    std::printf("\nDirect3DCreate9Ex                   hr=0x%08lX\n", hr);
    if (SUCCEEDED(hr) && d9ex != nullptr)
    {
        IDirect3DDevice9Ex *dev = nullptr;
        D3DPRESENT_PARAMETERS ex_pp = pp;
        hr = d9ex->CreateDeviceEx(D3DADAPTER_DEFAULT, D3DDEVTYPE_HAL, wnd,
            D3DCREATE_HARDWARE_VERTEXPROCESSING, &ex_pp, nullptr, &dev);
        std::printf("CreateDeviceEx                      hr=0x%08lX\n", hr);
        if (SUCCEEDED(hr)) { probe(dev, "D3D9Ex device"); dev->Release(); }
        d9ex->Release();
    }

    DestroyWindow(wnd);
    return 0;
}
