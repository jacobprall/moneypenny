class CreateUsers < ActiveRecord::Migration[5.2]
  def change
    create_table :users do |t|
      t.string :email, null: false
      t.string :fname, null: false 
      t.string :lname, null: false
      t.string :password_digest, null: false 
      t.string :session_token, null: false 
      t.timestamps
    end
    add_index "users", ["email"], name: "index_users_on_email", unique: true
  end
end
