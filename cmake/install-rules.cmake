install(
  TARGETS bulin_exe
  RUNTIME COMPONENT bulin_Runtime
)

if(PROJECT_IS_TOP_LEVEL)
  include(CPack)
endif()
