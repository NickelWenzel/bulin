out vec4 fragColor;

void main(){
  vec2 st = gl_FragCoord.xy/vec2(1000, 1000);
    vec3 color = vec3(0.5, 0.3, 0.0);

    float d = 0.25 + 0.5 * (0.5 + 0.5* sin(2.*time));

    vec2 pos = vec2(d)-st;

    float r = (2.+ sin(9. * time))/radius * length(pos);
    float a = atan(pos.y,pos.x);

    float f = abs(cos(a*2.5 + speed * time))*.5+.3;

  float ratio = smoothstep(f,f+0.02,r);
    color =  flower_color * (1.-ratio);
  vec3 back = vec3(1.) * ratio;

  // Visualize the distance field
  fragColor = vec4(color + back, 1.0);
}
