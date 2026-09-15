from pathlib import Path

from kohdalab_iv.api import config as config_module
from kohdalab_iv.api.config import (
    DEFAULT_CONFIG_PATH,
    managed_default_config_path,
    read_last_config_path,
    resolve_config_path,
    write_last_config_path,
)


def test_write_and_read_last_config_path(tmp_path):
    state_path = tmp_path / "state" / "last_config.json"
    config_path = tmp_path / "config.json"

    write_last_config_path(config_path, path=state_path)

    assert read_last_config_path(state_path) == config_path


def test_resolve_config_path_uses_last_existing_path(tmp_path):
    state_path = tmp_path / "last_config.json"
    config_path = tmp_path / "current.json"
    default_path = tmp_path / "default.json"
    config_path.write_text("{}", encoding="utf-8")
    default_path.write_text("{}", encoding="utf-8")
    write_last_config_path(config_path, path=state_path)

    resolution = resolve_config_path(
        last_state_path=state_path, lab_default_path=default_path
    )

    assert resolution.path == Path(config_path)
    assert resolution.source == "last"


def test_shared_state_paths_use_iv_specific_filenames(monkeypatch, tmp_path):
    shared_state_dir = tmp_path / "shared"
    iv_state_dir = tmp_path / "iv"
    monkeypatch.setenv("KOHDALAB_STATE_DIR", str(shared_state_dir))
    assert config_module.config_state_dir() == shared_state_dir
    assert config_module.user_config_dir() == shared_state_dir / "config"
    assert (
        config_module.last_config_state_path()
        == shared_state_dir / "last_iv_config.json"
    )

    monkeypatch.setenv(config_module.CONFIG_STATE_DIR_ENV, str(iv_state_dir))
    assert config_module.config_state_dir() == iv_state_dir
    assert (
        config_module.last_config_state_path() == iv_state_dir / "last_iv_config.json"
    )


def test_managed_default_is_editable_and_does_not_overwrite(monkeypatch, tmp_path):
    monkeypatch.setenv(config_module.CONFIG_STATE_DIR_ENV, str(tmp_path / "state"))

    managed = managed_default_config_path()
    assert managed == tmp_path / "state" / "config" / "iv.json"
    assert managed.read_bytes() == DEFAULT_CONFIG_PATH.read_bytes()

    managed.write_text('{"custom": true}', encoding="utf-8")
    assert (
        managed_default_config_path().read_text(encoding="utf-8") == '{"custom": true}'
    )


def test_legacy_last_config_is_used_only_without_state_override(monkeypatch, tmp_path):
    home = tmp_path / "home"
    legacy_config = tmp_path / "legacy-config.json"
    legacy_config.write_text("{}", encoding="utf-8")
    legacy_state = home / ".kohdalab-iv" / "last_config.json"
    write_last_config_path(legacy_config, legacy_state)
    monkeypatch.setattr(config_module.Path, "home", lambda: home)

    resolution = resolve_config_path(lab_default_path=tmp_path / "missing.json")
    assert resolution.path == legacy_config
    assert resolution.source == "legacy_last"

    monkeypatch.setenv(config_module.CONFIG_STATE_DIR_ENV, str(tmp_path / "state"))
    resolution = resolve_config_path(lab_default_path=tmp_path / "missing.json")
    assert resolution.path is None
    assert resolution.source == "none"


def test_path_inputs_expand_home(monkeypatch, tmp_path):
    home = tmp_path / "home"
    target = home / "config.json"
    monkeypatch.setenv("HOME", str(home))
    write_last_config_path("~/config.json", "~/state.json")

    assert read_last_config_path("~/state.json") == target


def test_missing_legacy_target_falls_back_to_default(monkeypatch, tmp_path):
    monkeypatch.setattr(config_module.Path, "home", lambda: tmp_path)
    write_last_config_path(
        tmp_path / "deleted.json", tmp_path / ".kohdalab-iv" / "last_config.json"
    )
    resolution = resolve_config_path()
    assert resolution.path == DEFAULT_CONFIG_PATH
    assert resolution.source == "lab_default"
