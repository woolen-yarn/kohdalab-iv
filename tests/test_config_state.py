from pathlib import Path

from kohdalab_iv.api.config import read_last_config_path, resolve_config_path, write_last_config_path


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

    resolution = resolve_config_path(last_state_path=state_path, lab_default_path=default_path)

    assert resolution.path == Path(config_path)
    assert resolution.source == "last"
