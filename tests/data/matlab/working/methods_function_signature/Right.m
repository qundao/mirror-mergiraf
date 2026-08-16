classdef BasicClass
   properties
      Value {mustBeNumeric}
   end
   methods
      r = ceil(obj)
      r = multiplyBy(obj,n)
   end
end