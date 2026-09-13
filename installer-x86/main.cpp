#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include "platform.h"
#include <commdlg.h>
#include <cmath>

// Presentation only. Installation, validation, journaling and uninstall remain in core.h.
namespace {
constexpr COLORREF background=RGB(18,20,23), panel=RGB(25,28,32), border=RGB(54,59,66);
constexpr COLORREF foreground=RGB(236,239,244), muted=RGB(166,174,186), red=RGB(235,32,64);
HWND targetBox,browseButton,cards[3],installButton,uninstallButton,detailsButton;
HFONT bodyFont,titleFont,smallFont,brandFont,brandBodyFont;
HBRUSH editBrush;
std::filesystem::path release;
std::wstring status=L"Ready to install.",details;
int selectedPreset=0;
float uiScale=1.0f;
constexpr int pageWidth=1100;
int pageHeight(){return selectedPreset==0?614:798;}
int px(int v){return static_cast<int>(std::lround(v*uiScale));}
std::wstring wide(const std::string& s){if(s.empty())return {};int n=MultiByteToWideChar(CP_UTF8,0,s.c_str(),-1,nullptr,0);std::wstring w(n,0);MultiByteToWideChar(CP_UTF8,0,s.c_str(),-1,w.data(),n);w.resize(n-1);return w;}
std::wstring value(HWND w){int n=GetWindowTextLengthW(w);std::wstring s(n+1,0);GetWindowTextW(w,s.data(),n+1);s.resize(n);return s;}
RECT box(int x,int y,int width,int height){return {px(x),px(y),px(x+width),px(y+height)};}
void text(HDC dc,const wchar_t* s,int x,int y,int width,int height,HFONT font,COLORREF colour,UINT format=DT_LEFT|DT_TOP|DT_NOPREFIX){
    auto old=SelectObject(dc,font);SetTextColor(dc,colour);SetBkMode(dc,TRANSPARENT);
    auto r=box(x,y,width,height);DrawTextW(dc,s,-1,&r,format);SelectObject(dc,old);
}
void rectangle(HDC dc,int x,int y,int width,int height,COLORREF fill,COLORREF edge,int thickness=1){
    auto brush=CreateSolidBrush(fill);auto pen=CreatePen(PS_SOLID,px(thickness)>0?px(thickness):1,edge);
    auto ob=SelectObject(dc,brush),op=SelectObject(dc,pen);
    RoundRect(dc,px(x),px(y),px(x+width),px(y+height),px(7),px(7));
    SelectObject(dc,ob);SelectObject(dc,op);DeleteObject(brush);DeleteObject(pen);
}
void line(HDC dc,int x,int y,int x2,int y2,COLORREF colour,int thickness=1){
    auto pen=CreatePen(PS_SOLID,px(thickness)>0?px(thickness):1,colour);auto old=SelectObject(dc,pen);
    MoveToEx(dc,px(x),px(y),nullptr);LineTo(dc,px(x2),px(y2));SelectObject(dc,old);DeleteObject(pen);
}
void polygon(HDC dc,POINT* points,int count,COLORREF fill){
    for(int i=0;i<count;++i){points[i].x=px(points[i].x);points[i].y=px(points[i].y);}
    auto brush=CreateSolidBrush(fill);auto old=SelectObject(dc,brush);auto pen=SelectObject(dc,GetStockObject(NULL_PEN));
    Polygon(dc,points,count);SelectObject(dc,old);SelectObject(dc,pen);DeleteObject(brush);
}
void heading(HDC dc,int number,const wchar_t* label,int y){
    rectangle(dc,286,y,790,number==1?160:number==2?248:176,panel,border);
    auto brush=CreateSolidBrush(RGB(40,44,50));auto old=SelectObject(dc,brush);auto pen=SelectObject(dc,GetStockObject(NULL_PEN));
    Ellipse(dc,px(308),px(y+20),px(346),px(y+58));SelectObject(dc,old);SelectObject(dc,pen);DeleteObject(brush);
    wchar_t digit[2]={static_cast<wchar_t>(L'0'+number),0};
    text(dc,digit,308,y+25,38,30,titleFont,foreground,DT_CENTER|DT_TOP|DT_NOPREFIX);
    text(dc,label,362,y+24,672,30,titleFont,foreground);
}
void paint(HWND window,HDC dc){
    RECT client;GetClientRect(window,&client);auto bg=CreateSolidBrush(background);FillRect(dc,&client,bg);DeleteObject(bg);
    auto side=box(0,0,260,pageHeight());auto sb=CreateSolidBrush(RGB(14,14,18));FillRect(dc,&side,sb);DeleteObject(sb);
    // Original, lightweight geometric art; no AMD/Radeon marks or assets.
    const int h=pageHeight();
    POINT a[]={{0,330},{42,330},{260,548},{260,634},{0,374}};polygon(dc,a,5,RGB(69,18,30));
    POINT b[]={{0,h-150},{260,h-410},{260,h-308},{0,h-48}};polygon(dc,b,4,RGB(35,19,26));
    line(dc,0,332,260,592,red,2);line(dc,0,h-145,260,h-405,RGB(125,24,43),2);
    for(int i=0;i<9;++i)line(dc,0,385+i*7,260,645+i*7,RGB(37,21,29));
    line(dc,259,0,259,h,border);
    text(dc,L"DLSS",28,67,150,50,brandFont,foreground);text(dc,L"5",166,67,55,50,brandFont,red);
    text(dc,L"Neural Rendering\non AMD",28,126,222,72,brandBodyFont,foreground,DT_LEFT|DT_WORDBREAK|DT_NOPREFIX);
    line(dc,28,218,92,218,red,3);
    text(dc,L"native x86 bridge for\nD3D11 / D3D9",28,244,222,62,smallFont,muted,DT_LEFT|DT_WORDBREAK|DT_NOPREFIX);
    text(dc,L"INDEPENDENT\nCOMMUNITY PROJECT",28,h-64,218,48,smallFont,muted,DT_LEFT|DT_WORDBREAK|DT_NOPREFIX);
    text(dc,L"A COMMUNITY PROJECT",286,25,790,20,smallFont,muted,DT_RIGHT|DT_TOP|DT_NOPREFIX);
    heading(dc,1,L"Game Executable",66);
    text(dc,L"Select the game's main executable (e.g. game.exe).",362,124,690,26,bodyFont,muted);
    rectangle(dc,310,164,610,42,RGB(16,18,21),border);
    heading(dc,2,L"Rendering API Preset",246);
    text(dc,L"Select the DirectX version used by the game.",362,304,690,26,bodyFont,muted);
    if(selectedPreset!=0){
        heading(dc,3,L"Important for D3D9",514);
        text(dc,L"!",310,583,38,44,brandBodyFont,RGB(255,183,69),DT_CENTER|DT_TOP|DT_NOPREFIX);
        text(dc,L"Install the official ReShade version with Full Add-on Support and select",362,580,690,28,bodyFont,foreground);
        text(dc,L"DirectX 9",362,608,220,26,titleFont,RGB(255,183,69));
        text(dc,L"during ReShade installation. No translation wrapper is required.",362,640,690,28,bodyFont,muted);
    }
    line(dc,286,h-96,1076,h-96,border);
    text(dc,status.c_str(),286,h-60,384,48,bodyFont,foreground,DT_LEFT|DT_TOP|DT_WORDBREAK|DT_END_ELLIPSIS|DT_NOPREFIX);
}
void drawButton(const DRAWITEMSTRUCT& d){
    auto dc=d.hDC;const int id=static_cast<int>(d.CtlID);
    const bool card=id>=20&&id<=22,chosen=card&&selectedPreset==id-20;
    const bool disabled=(d.itemState&ODS_DISABLED)!=0,pressed=(d.itemState&ODS_SELECTED)!=0;
    COLORREF fill=chosen?RGB(53,26,35):RGB(32,36,41);
    if(id==12)fill=RGB(210,23,53);if(pressed)fill=id==12?RGB(175,18,44):RGB(62,38,47);
    if(disabled)fill=RGB(32,33,36);
    int width=static_cast<int>(std::lround((d.rcItem.right-d.rcItem.left)/uiScale));
    int height=static_cast<int>(std::lround((d.rcItem.bottom-d.rcItem.top)/uiScale));
    rectangle(dc,0,0,width,height,fill,chosen?red:border,chosen?2:1);
    if(card){
        auto colour=chosen?red:RGB(120,129,141);int mid=width/2;
        rectangle(dc,mid-19,22,38,26,fill,colour,2);line(dc,mid,48,mid,56,colour,2);line(dc,mid-10,56,mid+10,56,colour,2);
        const wchar_t* labels[]={L"D3D11 x86",L"D3D9 x86",L"D3D8 unavailable"};
        text(dc,labels[id-20],0,69,width,28,titleFont,disabled?muted:foreground,DT_CENTER|DT_TOP|DT_NOPREFIX);
        auto brush=CreateSolidBrush(fill);auto pen=CreatePen(PS_SOLID,px(2)>0?px(2):1,colour);auto ob=SelectObject(dc,brush),op=SelectObject(dc,pen);
        Ellipse(dc,px(mid-9),px(108),px(mid+9),px(126));
        if(chosen){auto inner=CreateSolidBrush(foreground);SelectObject(dc,inner);Ellipse(dc,px(mid-4),px(113),px(mid+4),px(121));SelectObject(dc,brush);DeleteObject(inner);}
        SelectObject(dc,ob);SelectObject(dc,op);DeleteObject(brush);DeleteObject(pen);
    }else{
        auto label=value(d.hwndItem);text(dc,label.c_str(),0,0,width,height,bodyFont,disabled?muted:foreground,DT_CENTER|DT_VCENTER|DT_SINGLELINE|DT_NOPREFIX);
    }
    if(d.itemState&ODS_FOCUS){RECT focus=d.rcItem;InflateRect(&focus,-px(5),-px(5));DrawFocusRect(dc,&focus);}
}
void place(HWND w,int x,int y,int width,int height){MoveWindow(w,px(x),px(y),px(width),px(height),TRUE);}
void layout(HWND window){
    RECT outer=box(0,0,pageWidth,pageHeight());AdjustWindowRectEx(&outer,static_cast<DWORD>(GetWindowLongPtrW(window,GWL_STYLE)),FALSE,0);
    SetWindowPos(window,nullptr,0,0,outer.right-outer.left,outer.bottom-outer.top,SWP_NOMOVE|SWP_NOZORDER);
    // Keep the expanded notice within the current monitor, without adopting reference desktop geometry.
    MONITORINFO mi{sizeof(mi)};RECT actual{};GetWindowRect(window,&actual);
    if(GetMonitorInfoW(MonitorFromWindow(window,MONITOR_DEFAULTTONEAREST),&mi)){
        int x=actual.left,y=actual.top,w=actual.right-actual.left,h=actual.bottom-actual.top;
        if(x+w>mi.rcWork.right)x=mi.rcWork.right-w;if(y+h>mi.rcWork.bottom)y=mi.rcWork.bottom-h;
        x=(std::max)(x,static_cast<int>(mi.rcWork.left));y=(std::max)(y,static_cast<int>(mi.rcWork.top));
        SetWindowPos(window,nullptr,x,y,0,0,SWP_NOSIZE|SWP_NOZORDER);
    }
    place(targetBox,322,174,586,24);place(browseButton,932,164,120,42);
    for(int i=0;i<3;++i)place(cards[i],310+i*254,344,236,134);
    place(detailsButton,684,pageHeight()-70,100,44);place(uninstallButton,796,pageHeight()-70,124,44);place(installButton,932,pageHeight()-70,144,44);
    InvalidateRect(window,nullptr,TRUE);
}
void busy(HWND window,bool on){
    for(auto button:{browseButton,cards[0],cards[1],installButton,uninstallButton,detailsButton,targetBox})EnableWindow(button,!on);
    if(on)status=L"Working. Please wait...";InvalidateRect(window,nullptr,FALSE);UpdateWindow(window);
}
HWND button(HWND window,const wchar_t* label,int id){
    return CreateWindowW(L"BUTTON",label,WS_CHILD|WS_VISIBLE|WS_TABSTOP|BS_OWNERDRAW,0,0,1,1,window,reinterpret_cast<HMENU>(static_cast<INT_PTR>(id)),nullptr,nullptr);
}
LRESULT CALLBACK proc(HWND window,UINT msg,WPARAM w,LPARAM l){
    if(msg==WM_CREATE){
        targetBox=CreateWindowW(L"EDIT",L"",WS_CHILD|WS_VISIBLE|WS_TABSTOP|ES_AUTOHSCROLL,0,0,1,1,window,nullptr,nullptr,nullptr);
        SendMessageW(targetBox,WM_SETFONT,reinterpret_cast<WPARAM>(bodyFont),TRUE);
        browseButton=button(window,L"Browse...",10);
        const wchar_t* names[]={L"D3D11 x86 (selected)",L"D3D9 x86",L"D3D8 unavailable"};
        for(int i=0;i<3;++i)cards[i]=button(window,names[i],20+i);
        EnableWindow(cards[2],FALSE);
        detailsButton=button(window,L"Details",14);uninstallButton=button(window,L"Uninstall",13);installButton=button(window,L"Install",12);
        layout(window);return 0;
    }
    if(msg==WM_ERASEBKGND)return 1;
    if(msg==WM_PAINT){
        PAINTSTRUCT ps;auto dc=BeginPaint(window,&ps);RECT r;GetClientRect(window,&r);
        auto mem=CreateCompatibleDC(dc);auto bitmap=CreateCompatibleBitmap(dc,r.right,r.bottom);
        if(mem&&bitmap){auto old=SelectObject(mem,bitmap);paint(window,mem);BitBlt(dc,0,0,r.right,r.bottom,mem,0,0,SRCCOPY);SelectObject(mem,old);}else paint(window,dc);
        if(bitmap)DeleteObject(bitmap);if(mem)DeleteDC(mem);EndPaint(window,&ps);return 0;
    }
    if(msg==WM_DRAWITEM){drawButton(*reinterpret_cast<const DRAWITEMSTRUCT*>(l));return TRUE;}
    if(msg==WM_CTLCOLOREDIT){auto dc=reinterpret_cast<HDC>(w);SetTextColor(dc,foreground);SetBkColor(dc,RGB(16,18,21));return reinterpret_cast<LRESULT>(editBrush);}
    if(msg==WM_COMMAND){auto id=LOWORD(w);
        if(id>=20&&id<=21){selectedPreset=id-20;
            const wchar_t* labels[]={L"D3D11 x86",L"D3D9 x86",L"D3D8 unavailable"};
            for(int i=0;i<3;++i){std::wstring name=labels[i];if(i==selectedPreset)name+=L" (selected)";SetWindowTextW(cards[i],name.c_str());InvalidateRect(cards[i],nullptr,TRUE);}
            layout(window);return 0;
        }
        if(id==14){MessageBoxW(window,details.empty()?L"Close the game before installation. Select its actual x86 executable. Existing files are backed up; personal tuning is preserved.":details.c_str(),L"Installation details",MB_OK|MB_ICONINFORMATION);return 0;}
        if(id==10){wchar_t file[32768]{};OPENFILENAMEW of{sizeof(of)};of.hwndOwner=window;of.lpstrFilter=L"Executable (*.exe)\0*.exe\0\0";of.lpstrFile=file;of.nMaxFile=32768;of.Flags=OFN_FILEMUSTEXIST|OFN_PATHMUSTEXIST;if(GetOpenFileNameW(&of))SetWindowTextW(targetBox,file);return 0;}
        if(id==12||id==13){
            install86::Installer app;app.release=release;
            MONITORINFO mi{sizeof(mi)};if(GetMonitorInfoW(MonitorFromWindow(window,MONITOR_DEFAULTTONEAREST),&mi)){app.width=mi.rcWork.right-mi.rcWork.left;app.height=mi.rcWork.bottom-mi.rcWork.top;}
            bool failed=false;
            try {auto text=value(targetBox);install86::require(!text.empty(),"Select the target executable first");std::filesystem::path target=text;
                if(id==12){install86::require(install86::machine(install86::read(target))==0x14c,"x64 target refused: select an x86 executable");
                    busy(window,true);auto i=selectedPreset;
                    app.install(target,i==0?"D3D11":"D3D9");}
                else {if(MessageBoxW(window,L"Remove files owned by this installation and restore unchanged backups? Personal or modified configuration files will be kept.",L"Uninstall x86 bridge",MB_YESNO|MB_ICONQUESTION)!=IDYES)return 0;busy(window,true);app.uninstall(install86::installDirectory(target));}
            }catch(const std::exception& e){failed=true;app.note(std::string("ERROR: ")+e.what());}
            std::string result;for(const auto& entry:app.log)result+=entry+"\r\n";details=wide(result);
            status=failed?L"Could not complete. See Details.":L"Finished. Review Details for results.";busy(window,false);
            try{install86::write(release/L"installer-x86.log",install86::bytes(result));}catch(...){}
            return 0;
        }
    }
    if(msg==WM_DESTROY){PostQuitMessage(0);return 0;}return DefWindowProcW(window,msg,w,l);
}
HFONT font(int size,int weight){return CreateFontW(-px(size),0,0,0,weight,FALSE,FALSE,FALSE,DEFAULT_CHARSET,OUT_DEFAULT_PRECIS,CLIP_DEFAULT_PRECIS,CLEARTYPE_QUALITY,DEFAULT_PITCH|FF_DONTCARE,L"Segoe UI");}
}
int WINAPI wWinMain(HINSTANCE instance,HINSTANCE,LPWSTR,int show){
    wchar_t path[32768]{};GetModuleFileNameW(nullptr,path,32768);release=std::filesystem::path(path).parent_path();
    SetProcessDPIAware();auto dc=GetDC(nullptr);uiScale=static_cast<float>(GetDeviceCaps(dc,LOGPIXELSX))/96.0f;ReleaseDC(nullptr,dc);
    RECT work{};if(SystemParametersInfoW(SPI_GETWORKAREA,0,&work,0))uiScale=(std::min)(uiScale,(std::min)((work.right-work.left-40)/1100.0f,(work.bottom-work.top-80)/798.0f));
    bodyFont=font(16,FW_NORMAL);titleFont=font(21,FW_SEMIBOLD);smallFont=font(14,FW_NORMAL);brandFont=font(42,FW_BOLD);brandBodyFont=font(23,FW_SEMIBOLD);editBrush=CreateSolidBrush(RGB(16,18,21));
    WNDCLASSW cls{};cls.hInstance=instance;cls.lpfnWndProc=proc;cls.lpszClassName=L"DLSS5X86Installer";cls.hCursor=LoadCursor(nullptr,IDC_ARROW);RegisterClassW(&cls);
    HWND window=CreateWindowW(cls.lpszClassName,L"DLSS 5 Neural Rendering on AMD \u2014 Installer",WS_OVERLAPPED|WS_CAPTION|WS_SYSMENU|WS_MINIMIZEBOX|WS_CLIPCHILDREN,CW_USEDEFAULT,CW_USEDEFAULT,px(pageWidth),px(pageHeight()),nullptr,nullptr,instance,nullptr);
    int result=1;if(window){ShowWindow(window,show);MSG msg{};while(GetMessageW(&msg,nullptr,0,0)>0){if(!IsDialogMessageW(window,&msg)){TranslateMessage(&msg);DispatchMessageW(&msg);}}result=static_cast<int>(msg.wParam);}
    for(auto f:{bodyFont,titleFont,smallFont,brandFont,brandBodyFont})DeleteObject(f);DeleteObject(editBrush);return result;
}
