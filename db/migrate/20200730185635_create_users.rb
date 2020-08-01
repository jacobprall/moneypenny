class CreateUsers < ActiveRecord::Migration[5.2]
  def change
    create_table :users do |t|
      t.string "email", null: false
      t.string "password_digest", null: false 
      t.string "session_token", null:false 
      t.datetime "created_at"
      t.datetime "updated_at"
      t.string "avatar_file_name"
      t.string "avatar_content_type"
      t.string "fname"
      t.string "lname"
      t.string "gender"
      t.string "provider"
      t.string "uid"
      t.timestamps
    end
    add_index "users", ["email"], name: "index_users_on_email", unique: true, using: :btree
    add_index :users, [:provider, :uid], name: :index_users_on_provider_and_uid, unique: true, using: :btree 
  end
end
