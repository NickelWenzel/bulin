//
// Created by nickel on 12/1/24.
//

#include <bulin/model/cereal.hpp>
#include <bulin/model/model.hpp>

#include <lager/extra/cereal/inline.hpp>
#include <lager/extra/cereal/struct.hpp>
#include <lager/extra/cereal/immer_map.hpp>
#include <lager/extra/cereal/variant_with_name.hpp>

#include <cereal/archives/json.hpp>

#include <fstream>

namespace bulin
{
void save(std::filesystem::path const& fname, model state)
{
  auto stream = std::ofstream {fname};
  stream.exceptions(std::fstream::badbit | std::fstream::failbit);
  {
    auto archive = cereal::JSONOutputArchive {stream};
    save_inline(archive, state);
  }
}

model load(std::filesystem::path const& fname)
{
  auto stream = std::ifstream {fname};
  stream.exceptions(std::fstream::badbit);
  auto loaded_state = model {};
  {
    auto archive = cereal::JSONInputArchive {stream};
    load_inline(archive, loaded_state);
  }
  return loaded_state;
}
}  // namespace bulin
