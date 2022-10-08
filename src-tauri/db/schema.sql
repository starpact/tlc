CREATE TABLE IF NOT EXISTS settings (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    save_root_dir TEXT NOT NULL,
    video_metadata TEXT NOT NULL, -- JSON
    daq_metadata TEXT NOT NULL, -- JSON
    start_frame INTEGER,
    start_row INTEGER,
    area TEXT, -- JSON
    thermocouples TEXT, -- JSON
    filter_method TEXT NOT NULL, -- JSON
    iteration_method TEXT NOT NULL, -- JSON
    peak_temperature REAL NOT NULL,
    solid_thermal_conductivity REAL NOT NULL,
    solid_thermal_diffusivity REAL NOT NULL,
    characteristic_length REAL NOT NULL,
    air_thermal_conductivity REAL NOT NULL,
    completed INTEGER NOT NULL, -- bool
    created_at INTEGER NOT NULL, -- timestamp in milliseconds
    updated_at INTEGER NOT NULL -- timestamp in milliseconds
);
