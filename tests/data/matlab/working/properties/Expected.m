classdef BasicClass
   properties
      Value {mustBeNumeric}
      Value2
      Value3
   end
   methods
      function r = roundOff(obj)
         r = round([obj.Value],2);
      end
      function r = multiplyBy(obj,n)
         r = [obj.Value]*n;
      end
   end
end