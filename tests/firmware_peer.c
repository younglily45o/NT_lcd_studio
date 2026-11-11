/* Simulated hardware boundary for the portable production protocol core. */
#include "lcs_protocol.h"
#include <stdio.h>
#include <string.h>

typedef struct {LcsSettings settings;int mismatch;} Hardware;
static int read_settings(void *context,LcsSettings *out)
{*out=((Hardware *)context)->settings;return 1;}
static int apply_settings(void *context,const LcsSettings *value)
{
    Hardware *hardware=context;hardware->settings=*value;
    if(hardware->mismatch)hardware->settings.color.ccon^=1;
    return 1;
}
static void emit(void *context,const char *line)
{(void)context;printf("%s\r\n",line);fflush(stdout);}
int main(int argc,char **argv)
{
    Hardware hardware={0};LcsEngine engine={0};char line[128];size_t i;
    const LcsIo io={&hardware,read_settings,apply_settings,emit};
    hardware.mismatch=argc>1&&!strcmp(argv[1],"mismatch");
    hardware.settings.gamma.enabled=hardware.settings.color.enabled=1;
    hardware.settings.color.ycon=hardware.settings.color.ccon=128;
    for(i=0;i<33;i++)hardware.settings.gamma.red[i]=hardware.settings.gamma.green[i]=hardware.settings.gamma.blue[i]=(uint8_t)(i==32?255:i*8);
    while(fgets(line,sizeof(line),stdin)){
        const char *args[8];int count=0;char *word=strtok(line," \t\r\n");
        while(word&&count<8){args[count++]=word;word=strtok(NULL," \t\r\n");}
        if(count>=4&&!strcmp(args[0],"key")&&!strcmp(args[1],"lcdiq"))lcs_handle(&engine,count-2,args+2,&io);
    }
    return 0;
}
