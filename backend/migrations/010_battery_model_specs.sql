-- Battery model reference data (system-level — no org_id).
-- Nominal cycle counts come from the manufacturer's user manuals; uncertainty
-- margin is a conservative +/- to account for real-world variance. Used by the
-- fleet statistics service to compute estimated remaining lifespan ranges.

CREATE TABLE IF NOT EXISTS battery_model_specs (
    model               TEXT PRIMARY KEY,
    nominal_max_cycles  INTEGER NOT NULL,
    cycle_uncertainty   INTEGER NOT NULL,
    nominal_capacity_wh REAL,
    source              TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO battery_model_specs (model, nominal_max_cycles, cycle_uncertainty, nominal_capacity_wh, source) VALUES
    ('DJI Neo Battery',                    200, 40,  17.5,   'DJI user manual'),
    ('DJI Air 2s Intelligent Battery',     200, 40,  40.4,   'DJI user manual'),
    ('DJI Mavic 2 Intelligent Battery',    200, 40,  59.29,  'DJI user manual'),
    ('DJI Matrice 600 TB47S',              200, 40, 129.96,  'DJI user manual'),
    ('DJI Inspire 1 TB47/TB48',            200, 40, 129.96,  'DJI user manual')
ON CONFLICT (model) DO NOTHING;

-- Partial index: per-model aggregations only care about active batteries.
CREATE INDEX IF NOT EXISTS idx_batteries_model_active
    ON batteries(compatible_model)
    WHERE status = 'active';

-- Speeds up controller-per-aircraft breakdowns.
CREATE INDEX IF NOT EXISTS idx_flights_controller_aircraft
    ON flights(controller_id, aircraft_id)
    WHERE deleted_at IS NULL;

-- Speeds up battery usage lookups.
CREATE INDEX IF NOT EXISTS idx_flights_battery
    ON flights(battery_id)
    WHERE deleted_at IS NULL;
