classdef BasicClass
   properties
      Value {mustBeNumeric}
   end
   methods
      r = roundOff(obj)
      r = ceil(obj)
      r = multiplyBy(obj,n)
   end
end