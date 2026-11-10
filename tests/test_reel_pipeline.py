import json, sys, tempfile, unittest, subprocess
from pathlib import Path
sys.path.insert(0,str(Path(__file__).parents[1]/'tools'))
import reel_analysis as r

class ReelFlowTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)/'Reels';self.root.mkdir()
        self.models=Path(self.temp.name)/'models';self.models.mkdir()
        self.media=self.root/'LegacyUser'/'ABC123.mp4';self.media.parent.mkdir()
        proc=subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-f','lavfi','-i','color=c=blue:s=160x90:r=3',
                             '-t','1','-vcodec','mpeg4','-y',str(self.media)],capture_output=True)
        if proc.returncode: raise RuntimeError(proc.stderr)
        (self.media.parent/'ABC123.info.json').write_text(json.dumps({
            'webpage_url':'https://www.instagram.com/reel/ABC123/','id':'ABC123',
            'description':'Pasta with tomato and garlic, an easy food recipe', 'uploader_id':'cook'
        }))
    def test_direct_end_to_end_safe(self):
        digest=r.sha256(self.media)
        result=r.analyze(self.root,'LegacyUser/ABC123.mp4',self.models)
        self.assertEqual(result['id'],'ABC123')
        normalized=self.root/result['video_relative']
        self.assertTrue(normalized.samefile(self.media))
        self.assertFalse(r.approved_for_unsave(normalized.parent,self.root,'https://www.instagram.com/reel/ABC123/'))
        r.record_category(self.root,result['video_relative'],'Food and recipes',0.83)
        a=r.read_json(normalized.parent/'analysis.json')
        self.assertEqual(a['status'],'needs_review')
        self.assertFalse(r.approved_for_unsave(normalized.parent,self.root,a['source_url']))
        organized=r.approve(self.root,result['video_relative'],'Food and recipes')
        self.assertEqual(organized['status'],'approved')
        self.assertTrue(r.approved_for_unsave(normalized.parent,self.root,a['source_url']))
        self.assertEqual(r.sha256(self.media),digest)
        self.assertEqual(len(list(self.root.glob('Organized/**/video.mp4'))),1)
    def test_changed_bytes_blocks_approval(self):
        result=r.analyze(self.root,'LegacyUser/ABC123.mp4',self.models)
        normalized=self.root/result['video_relative']
        with normalized.open('ab') as f:f.write(b'CORRUPTED')
        with self.assertRaises(ValueError):r.approve(self.root,result['video_relative'],'Other')
        self.assertFalse(r.approved_for_unsave(normalized.parent,self.root,'https://www.instagram.com/reel/ABC123/'))
    def test_invalid_source_rejected(self):
        info=self.media.parent/'ABC123.info.json'
        meta=json.loads(info.read_text());meta['webpage_url']='https://evil.example/reel/ABC123/'
        info.write_text(json.dumps(meta))
        with self.assertRaises(ValueError):r.analyze(self.root,'LegacyUser/ABC123.mp4',self.models)
    def test_reanalysis_revokes_approval(self):
        result=r.analyze(self.root,'LegacyUser/ABC123.mp4',self.models)
        rel=result['video_relative'];normalized=self.root/rel
        r.record_category(self.root,rel,'Other',0.0);r.approve(self.root,rel,'Other')
        self.assertTrue(r.approved_for_unsave(normalized.parent,self.root,'https://www.instagram.com/reel/ABC123/'))
        r.analyze(self.root,rel,self.models)
        self.assertFalse(r.approved_for_unsave(normalized.parent,self.root,'https://www.instagram.com/reel/ABC123/'))
    def test_absolute_or_parent_paths_rejected(self):
        for path in ('../bad.mp4',str(self.media)):
            with self.assertRaises(ValueError):r.checked_media(self.root,path)
    def test_saved_collection_flow(self):
        folder=self.root/'Collections'/'Recipes-abcde'/'ABC123';folder.mkdir(parents=True)
        media=folder/'video.mp4';media.hardlink_to(self.media)
        manifest={'schema_version':2,'id':'ABC123','source_url':'https://www.instagram.com/reel/ABC123/',
                  'caption':'Food recipe','video_file':'video.mp4','size_bytes':media.stat().st_size,
                  'sha256':r.sha256(media),'downloaded':True,'collection':'Recipes','instagram_unsaved':False}
        r.write_json(folder/'manifest.json',manifest)
        analyzed=r.analyze(self.root,str(media.relative_to(self.root)),self.models)
        r.record_category(self.root,analyzed['video_relative'],'Food and recipes',0.9)
        self.assertFalse(r.approved_for_unsave(folder,self.root,manifest['source_url']))
        r.approve(self.root,analyzed['video_relative'],'Food and recipes')
        self.assertTrue(r.approved_for_unsave(folder,self.root,manifest['source_url']))
    def test_browser_never_opens_instagram_before_approval(self):
        from unittest.mock import Mock
        sys.path.insert(0,str(Path(__file__).parents[1]/'tools'))
        import instagram_saved as saved
        folder=self.root/'Collections'/'Recipes-abcde'/'ABC123';folder.mkdir(parents=True)
        media=folder/'video.mp4';media.hardlink_to(self.media)
        url='https://www.instagram.com/reel/ABC123/'
        r.write_json(folder/'manifest.json',{
            'schema_version':2,'id':'ABC123','source_url':url,'downloaded':True,
            'video_file':media.name,'size_bytes':media.stat().st_size,'sha256':r.sha256(media)
        })
        mock=Mock()
        self.assertFalse(saved.unsave_verified(mock,folder,url))
        mock.new_page.assert_not_called()

    def test_existing_organization_conflict_blocked(self):
        result=r.analyze(self.root,'LegacyUser/ABC123.mp4',self.models)
        path=self.root/'Organized'/'food-and-recipes'/'ABC123';path.mkdir(parents=True)
        (path/'video.mp4').write_bytes(b'not same')
        with self.assertRaises(ValueError):r.approve(self.root,result['video_relative'],'Food and recipes')

if __name__=='__main__':unittest.main()
