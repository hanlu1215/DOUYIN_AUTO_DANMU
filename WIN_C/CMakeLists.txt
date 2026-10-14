# 抖音直播自动弹幕助手（C++ 版）
#
# 构建（MinGW / MSYS2 UCRT64）：
#   cmake -B build -G "MinGW Makefiles" -DCMAKE_BUILD_TYPE=Release
#   cmake --build build
# 构建（Visual Studio）：
#   cmake -B build -G "Visual Studio 17 2022"
#   cmake --build build --config Release
#
# 产物 build/douyin_danmu.exe（VS 为 build/<Config>/douyin_danmu.exe），
# 已静态链接 C/C++ 运行时，只依赖 Windows 系统 DLL，可拷贝到其他 Win10/11 机器直接运行。
# exe 会自动向上查找含 danmu/ 的项目根目录（兼容 build/、build/Release/ 布局）。

cmake_minimum_required(VERSION 3.15)
project(douyin_danmu LANGUAGES CXX RC)

if(NOT WIN32)
    message(FATAL_ERROR "本项目仅支持 Windows（依赖 Win32 API）")
endif()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

add_compile_definitions(UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX
                        WINVER=0x0A00 _WIN32_WINNT=0x0A00)

set(SOURCES
    src/main.cpp
    src/app.cpp
    src/win_input.cpp
    src/sender.cpp
    src/config.cpp
    src/danmu_io.cpp
    src/json_mini.cpp
    src/app.rc          # 图标 + 清单（app.ico 由 tools/make_icon.py 生成）
)

add_executable(douyin_danmu WIN32 ${SOURCES})

target_link_libraries(douyin_danmu PRIVATE user32 gdi32 comctl32 comdlg32 shcore)

if(MSVC)
    # /utf-8：源文件为 UTF-8（界面全是中文）；/MT：静态链接运行时，免装 VC++ Redistributable
    target_compile_options(douyin_danmu PRIVATE /utf-8 /W4)
    set_property(TARGET douyin_danmu PROPERTY
        MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
else()
    target_compile_options(douyin_danmu PRIVATE -Wall -Wextra)
endif()

if(MINGW)
    # -municode 提供 wWinMain 入口；-static 静态链接 libgcc/libstdc++/winpthread，
    # 使 exe 只依赖系统 DLL（UCRT 为 Win10 系统组件），可直接拷贝运行
    target_link_options(douyin_danmu PRIVATE -municode -static)
endif()

set_target_properties(douyin_danmu PROPERTIES
    VS_DEBUGGER_WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}")
