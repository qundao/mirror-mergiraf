classdef BasicClass
    properties
        Value
    end
    methods
        function r = fRight(obj)
            r = sin(obj.Value);
        end
        function r = f1(obj,n)
            r = 2*n;
        end
    end
end