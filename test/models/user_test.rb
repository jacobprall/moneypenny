# == Schema Information
#
# Table name: users
#
#  id               :bigint           not null, primary key
#  avatar_file_name :string
#  email            :string           not null
#  fname            :string           not null
#  lname            :string           not null
#  password_digest  :string           not null
#  session_token    :string           not null
#  created_at       :datetime         not null
#  updated_at       :datetime         not null
#
# Indexes
#
#  index_users_on_email  (email) UNIQUE
#
require 'test_helper'

class UserTest < ActiveSupport::TestCase
  # test "the truth" do
  #   assert true
  # end
end
