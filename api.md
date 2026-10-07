# API

This is just for me to communicate my API ideas to you. it can be deleted when the documentation is properly set in its correct place.


## data updates

/api/v1/report

Methods: POST
Payload:
  * aircraft - The aircraft tail number. I know tail numbers are not unique in the real world, but in this simulation, we'll deal with it.

  These should be obvious:
  * latitude
  * longitude
  * altitude
  * heading (bearing?)
  * velocity
  * flight_id - A unique identifier for this flight

/api/v1/flight

Method: PUT/POST (updates)

  * source : Airport code
  * destination: Airport code
  * scheduled_departure: 
  * actual_departure: 
  * scheduled_arrival:
  * actual_arrival:
  * flight_id - 
  * flight - similar to DL1234
  * aircraft - the aircraft assigned to this flight

/api/v1/flight/search

Methods: GET

Search for flights by airport codes, dates, flight, aircraft, etc...
Basically, make it so i can say "show my all pending arrivals at this airport", or "show me the past flights for this aircraft"

## health

/healthz /readyz


## airports

/api/v1/airport

GET - list all airports in the simulation

/api/v1/airport/{code}

GET - get the airport details
POST - Update airport details
PUT - Add an airport

For now, we'll just have:
* code - e.g. MSP
* latitiude 
* longitude
* elevation - worthwhile in the simulation? do we just treat everything "on the ground" as sea level?
* name - e.g. Minneapolis/St. Paul International Airport 


## aircraft

/api/v1/aircraft

GET - list all aircraft in teh simuation

/api/v1/aircraft/{tail number}

GET - get aircraft detail
PUT - add an aircraft
POST - Update an aircraft

for now:

* registration - e.g. tail number
* type - e.g. A359


