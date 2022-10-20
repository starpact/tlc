CREATE TABLE IF NOT EXISTS settings (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    save_root_dir TEXT NOT NULL,
    video_metadata TEXT, -- JSON
    daq_metadata TEXT, -- JSON
    start_frame INTEGER,
    start_row INTEGER,
    area TEXT, -- JSON
    thermocouples TEXT, -- JSON
    interpolation_method TEXT, --JSON
    filter_method TEXT NOT NULL, -- JSON
    iteration_method TEXT NOT NULL, -- JSON
    gmax_temperature REAL NOT NULL,
    solid_thermal_conductivity REAL NOT NULL,
    solid_thermal_diffusivity REAL NOT NULL,
    characteristic_length REAL NOT NULL,
    air_thermal_conductivity REAL NOT NULL,
    completed_at INTEGER NOT NULL, -- timestamp in milliseconds
    created_at INTEGER NOT NULL, -- timestamp in milliseconds
    updated_at INTEGER NOT NULL -- timestamp in milliseconds
);
