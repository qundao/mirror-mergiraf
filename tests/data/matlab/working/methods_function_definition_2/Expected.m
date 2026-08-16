classdef BasicClass
    methods
        function r = f_left(obj)
            r = round([obj.Test]);
        end
        function r = f_right(obj)
            r = ceil([obj.Value],2);
        end
        function r = f_a(obj,n)
            r = 100*n;
        end
    end
end