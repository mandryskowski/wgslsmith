#pragma once

#include <memory>
#include <string>
#include <vector>

bool validate_shader(const char* source);

std::unique_ptr<std::string> compile_shader_to_hlsl(const char* source);

std::unique_ptr<std::string> compile_shader_to_msl(const char* source);

std::unique_ptr<std::vector<uint32_t>> compile_shader_to_spirv(const char* source);
