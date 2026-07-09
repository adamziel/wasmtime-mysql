cmake_minimum_required(VERSION 3.13)
project(mysql_wasi_host_protoc C CXX)

if(NOT DEFINED PROTOBUF_SOURCE_DIR)
  message(FATAL_ERROR "PROTOBUF_SOURCE_DIR is required")
endif()

function(ADD_OBJDUMP_TARGET)
endfunction()

add_subdirectory("${PROTOBUF_SOURCE_DIR}" protobuf)
