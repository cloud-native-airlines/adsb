-- A short, curated set of North American airports: the demo region (upper
-- Midwest, incl. RST) plus the largest US and Canadian hubs. Add other
-- geographies in their own 00x_seed_*.sql files.
INSERT INTO airports (code, name, latitude, longitude, icao, country) VALUES
    -- Demo region / upper Midwest
    ('MSP', 'Minneapolis-Saint Paul International', 44.8820, -93.2218, 'KMSP', 'US'),
    ('ORD', 'Chicago O''Hare International',         41.9742, -87.9073, 'KORD', 'US'),
    ('RST', 'Rochester International',               43.9083, -92.5000, 'KRST', 'US'),
    ('MDW', 'Chicago Midway International',          41.7868, -87.7522, 'KMDW', 'US'),
    ('MKE', 'Milwaukee Mitchell International',      42.9472, -87.8966, 'KMKE', 'US'),
    ('MSN', 'Dane County Regional (Madison)',        43.1399, -89.3375, 'KMSN', 'US'),
    ('GRB', 'Green Bay Austin Straubel Intl',        44.4851, -88.1296, 'KGRB', 'US'),
    ('DLH', 'Duluth International',                   46.8420, -92.1936, 'KDLH', 'US'),
    ('FSD', 'Sioux Falls Regional',                  43.5820, -96.7419, 'KFSD', 'US'),
    ('FAR', 'Hector International (Fargo)',           46.9207, -96.8158, 'KFAR', 'US'),
    ('DSM', 'Des Moines International',               41.5340, -93.6631, 'KDSM', 'US'),
    ('OMA', 'Eppley Airfield (Omaha)',               41.3032, -95.8941, 'KOMA', 'US'),
    ('MCI', 'Kansas City International',              39.2976, -94.7139, 'KMCI', 'US'),
    ('STL', 'St. Louis Lambert International',        38.7487, -90.3700, 'KSTL', 'US'),
    ('IND', 'Indianapolis International',             39.7173, -86.2944, 'KIND', 'US'),
    ('DTW', 'Detroit Metropolitan Wayne County',     42.2124, -83.3534, 'KDTW', 'US'),
    -- Largest US hubs
    ('ATL', 'Hartsfield-Jackson Atlanta Intl',       33.6407, -84.4277, 'KATL', 'US'),
    ('DFW', 'Dallas/Fort Worth International',        32.8998, -97.0403, 'KDFW', 'US'),
    ('DEN', 'Denver International',                   39.8561, -104.6737, 'KDEN', 'US'),
    ('LAX', 'Los Angeles International',              33.9416, -118.4085, 'KLAX', 'US'),
    ('SFO', 'San Francisco International',            37.6213, -122.3790, 'KSFO', 'US'),
    ('SEA', 'Seattle-Tacoma International',           47.4502, -122.3088, 'KSEA', 'US'),
    ('LAS', 'Harry Reid International (Las Vegas)',   36.0840, -115.1537, 'KLAS', 'US'),
    ('PHX', 'Phoenix Sky Harbor International',       33.4342, -112.0116, 'KPHX', 'US'),
    ('IAH', 'Houston George Bush Intercontinental',  29.9902, -95.3368, 'KIAH', 'US'),
    ('MCO', 'Orlando International',                  28.4312, -81.3081, 'KMCO', 'US'),
    ('MIA', 'Miami International',                    25.7959, -80.2870, 'KMIA', 'US'),
    ('CLT', 'Charlotte Douglas International',        35.2140, -80.9431, 'KCLT', 'US'),
    ('JFK', 'New York John F. Kennedy Intl',         40.6413, -73.7781, 'KJFK', 'US'),
    ('EWR', 'Newark Liberty International',           40.6895, -74.1745, 'KEWR', 'US'),
    ('BOS', 'Boston Logan International',             42.3656, -71.0096, 'KBOS', 'US'),
    -- Largest Canadian hubs
    ('YYZ', 'Toronto Pearson International',          43.6777, -79.6248, 'CYYZ', 'CA'),
    ('YUL', 'Montreal-Trudeau International',         45.4706, -73.7408, 'CYUL', 'CA'),
    ('YVR', 'Vancouver International',                49.1967, -123.1815, 'CYVR', 'CA'),
    ('YYC', 'Calgary International',                  51.1139, -114.0203, 'CYYC', 'CA'),
    ('YEG', 'Edmonton International',                 53.3097, -113.5800, 'CYEG', 'CA'),
    ('YWG', 'Winnipeg James Richardson Intl',         49.9100, -97.2399, 'CYWG', 'CA'),
    ('YOW', 'Ottawa Macdonald-Cartier International', 45.3225, -75.6692, 'CYOW', 'CA')
ON CONFLICT (code) DO NOTHING;
